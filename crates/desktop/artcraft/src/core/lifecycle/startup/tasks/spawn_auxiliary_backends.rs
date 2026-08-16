//! Starts the packaged MediaCrawler and OpenMontage backend sidecars.

use std::net::TcpStream;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};

use super::background_command::background_command;

struct BackendSpec {
  label: &'static str,
  directory: &'static str,
  executable: &'static str,
  port: u16,
}

const BACKENDS: [BackendSpec; 2] = [BackendSpec { label: "MediaCrawler", directory: "media-crawler", executable: "media-crawler-server.exe", port: 8080 }, BackendSpec { label: "OpenMontage", directory: "openmontage", executable: "openmontage-server.exe", port: 4750 }];

pub struct AuxiliaryBackendProcesses {
  children: Mutex<Vec<Child>>,
}

impl Drop for AuxiliaryBackendProcesses {
  fn drop(&mut self) {
    self.stop();
  }
}

impl AuxiliaryBackendProcesses {
  pub fn stop(&self) {
    if let Ok(mut children) = self.children.lock() {
      for child in children.iter_mut() {
        info!("Stopping auxiliary backend (pid={})", child.id());
        kill_process_tree(child);
        let _ = child.wait();
      }
      children.clear();
    }
  }
}

/// Terminates a sidecar together with anything it spawned.
///
/// `Child::kill` only signals the direct child, so a sidecar that forked its own
/// workers (MediaCrawler starts Playwright browsers) leaves them behind holding
/// handles on the staged executables.
fn kill_process_tree(child: &mut Child) {
  #[cfg(windows)]
  {
    let _ = background_command(Command::new("taskkill")).args(["/PID", &child.id().to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
  }
  let _ = child.kill();
}

fn port_open(port: u16) -> bool {
  let Ok(address) = format!("127.0.0.1:{port}").parse() else {
    return false;
  };
  TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok()
}

fn expected_backend_healthy(port: u16) -> bool {
  let Ok(address) = format!("127.0.0.1:{port}").parse() else {
    return false;
  };
  let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
    return false;
  };
  let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
  let request = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
  if stream.write_all(request.as_bytes()).is_err() {
    return false;
  }
  let mut response = String::new();
  if stream.read_to_string(&mut response).is_err() {
    return false;
  }
  let status_ok = response.starts_with("HTTP/1.1 2") || response.starts_with("HTTP/1.0 2");
  status_ok && response.contains("\"be\":\"capcut-mate\"")
}

fn push_unique(candidates: &mut Vec<PathBuf>, path: PathBuf) {
  if !candidates.iter().any(|candidate| candidate == &path) {
    candidates.push(path);
  }
}

fn resolve_sidecar(app: &AppHandle, spec: &BackendSpec) -> Option<PathBuf> {
  let mut candidates = Vec::new();

  if let Ok(executable) = std::env::current_exe() {
    if let Some(directory) = executable.parent() {
      push_unique(&mut candidates, directory.join(spec.executable));
      push_unique(&mut candidates, directory.join(spec.directory).join(spec.executable));
      push_unique(&mut candidates, directory.join("resources").join(spec.directory).join(spec.executable));
    }
  }

  if let Ok(resources) = app.path().resource_dir() {
    push_unique(&mut candidates, resources.join(spec.directory).join(spec.executable));
    push_unique(&mut candidates, resources.join("resources").join(spec.directory).join(spec.executable));
  }

  candidates.into_iter().find(|candidate| candidate.is_file())
}

fn spawn_sidecar(executable: &Path, spec: &BackendSpec, runtime_root: &Path) -> Result<Child, String> {
  let mut command = background_command(Command::new(executable));
  command.current_dir(runtime_root).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
  // The staged auxiliary executables are copies of the unified backend. Give
  // each copy its declared port so it cannot also bind the canonical :30000.
  command.env("UNIFIED_SERVER_PORT", spec.port.to_string());

  match spec.label {
    "MediaCrawler" => {
      command.env("MEDIACRAWLER_RUNTIME_DIR", runtime_root);
      command.env("MEDIACRAWLER_DATA_DIR", runtime_root.join("data"));
      if let Some(sidecar_dir) = executable.parent() {
        let browsers = sidecar_dir.join("ms-playwright");
        if browsers.is_dir() {
          command.env("PLAYWRIGHT_BROWSERS_PATH", browsers);
        }
      }
    },
    "OpenMontage" => {
      command.env("OPENMONTAGE_PROJECTS_DIR", runtime_root.join("projects"));
      command.env("BACKLOT_PORT", spec.port.to_string());
      let logs_dir = runtime_root.join("logs");
      std::fs::create_dir_all(&logs_dir).map_err(|error| format!("Cannot create OpenMontage log directory: {error}"))?;
      let stdout = OpenOptions::new().create(true).append(true).open(logs_dir.join("stdout.log")).map_err(|error| format!("Cannot open OpenMontage stdout log: {error}"))?;
      let stderr = OpenOptions::new().create(true).append(true).open(logs_dir.join("stderr.log")).map_err(|error| format!("Cannot open OpenMontage stderr log: {error}"))?;
      command.stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
    },
    _ => {},
  }

  command.spawn().map_err(|error| format!("Failed to start {}: {error}", spec.label))
}

/// Starts packaged sidecars when present. Missing sidecars never crash Artcraft.
pub fn spawn_auxiliary_backends(app: &AppHandle) {
  let app_data = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ArtCraft"));
  let mut children = Vec::new();

  for spec in &BACKENDS {
    if port_open(spec.port) {
      if expected_backend_healthy(spec.port) {
        info!("{} already listening on :{} with expected service identity", spec.label, spec.port);
      } else {
        warn!("{} PORT_IN_USE: :{} is occupied by an unexpected service; refusing reuse", spec.label, spec.port);
      }
      continue;
    }

    let Some(sidecar) = resolve_sidecar(app, spec) else {
      info!("{} sidecar not found; skipping packaged auto-start", spec.label);
      continue;
    };

    let runtime_root = app_data.join(spec.directory);
    if let Err(error) = std::fs::create_dir_all(&runtime_root) {
      warn!("Cannot create {} runtime directory {}: {}", spec.label, runtime_root.display(), error);
      continue;
    }

    match spawn_sidecar(&sidecar, spec, &runtime_root) {
      Ok(child) => {
        info!("Started {} sidecar {} (pid={}, port={})", spec.label, sidecar.display(), child.id(), spec.port);
        children.push(child);
      },
      Err(error) => warn!("{error}"),
    }
  }

  app.manage(AuxiliaryBackendProcesses { children: Mutex::new(children) });
}
