use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use log::{info, warn};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use super::background_command::background_command;

const HOST: &str = "127.0.0.1";
const PORT: u16 = 10108;
const READY_DEADLINE: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(400);
const PLAYWRIGHT_PORT: u16 = 9223;
static PLAYWRIGHT_CHILD: OnceLock<Arc<Mutex<Option<Child>>>> = OnceLock::new();
const REQUIRED_RUNTIME_ARTIFACTS: &[&str] = &[
  "donut-runtime/floword-donut-runtime.exe",
  "donut-runtime/bundled-extensions/chromex.zip",
  "node/node.exe",
  "playwright-sidecar/src/server.js",
  "playwright-sidecar/package.json",
  "playwright-sidecar/node_modules/express/package.json",
  "playwright-sidecar/node_modules/playwright/package.json",
  "chromex-extension/manifest.json",
];

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeSupervisorState {
  Stopped,
  Starting,
  Ready,
  Degraded,
  Stopping,
  Crashed,
  PortConflict,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeSupervisorStatus {
  pub state: RuntimeSupervisorState,
  pub pid: Option<u32>,
  pub base_url: String,
  pub last_error: Option<String>,
  pub started_at: Option<String>,
  pub last_health_at: Option<String>,
}

#[derive(Debug)]
struct RuntimeHealth {
  runtime: String,
  pid: Option<u32>,
  protocol: Option<String>,
  protocol_version: Option<u32>,
}

#[derive(Clone)]
pub struct RuntimeSupervisor {
  child: Arc<Mutex<Option<Child>>>,
  status: Arc<Mutex<RuntimeSupervisorStatus>>,
}

impl Default for RuntimeSupervisor {
  fn default() -> Self {
    Self { child: Arc::new(Mutex::new(None)), status: Arc::new(Mutex::new(RuntimeSupervisorStatus { state: RuntimeSupervisorState::Stopped, pid: None, base_url: format!("http://{HOST}:{PORT}"), last_error: None, started_at: None, last_health_at: None })) }
  }
}

impl Drop for RuntimeSupervisor {
  fn drop(&mut self) {
    self.stop();
    stop_playwright_runtime();
  }
}

impl RuntimeSupervisor {
  pub fn status(&self) -> RuntimeSupervisorStatus {
    self.status.lock().map(|status| status.clone()).unwrap_or(RuntimeSupervisorStatus { state: RuntimeSupervisorState::Degraded, pid: None, base_url: format!("http://{HOST}:{PORT}"), last_error: Some("runtime supervisor state lock poisoned".to_string()), started_at: None, last_health_at: None })
  }

  pub fn start(&self, app: &AppHandle) {
    if port_open() {
      match runtime_health() {
        Some(health) if health.protocol.as_deref() == Some("floword-production") && health.protocol_version == Some(1) => {
          if let Ok(mut status) = self.status.lock() {
            status.state = RuntimeSupervisorState::Ready;
            status.pid = health.pid;
            status.last_error = None;
            status.last_health_at = Some(chrono::Utc::now().to_rfc3339());
          }
          info!("Attached to existing Floword runtime {} (pid={:?}) on http://{HOST}:{PORT}", health.runtime, health.pid);
          return;
        },
        Some(health) => {
          self.set_failure(RuntimeSupervisorState::PortConflict, format!("127.0.0.1:{PORT} is occupied by incompatible runtime {} (protocol {:?} v{:?})", health.runtime, health.protocol, health.protocol_version));
          return;
        },
        None => {
          self.set_failure(RuntimeSupervisorState::PortConflict, "127.0.0.1:10108 is occupied by a process that does not expose a valid Floword runtime health endpoint");
          return;
        },
      }
    }

    let Some(executable) = resolve_runtime_executable(app) else {
      info!("Floword Donut runtime executable not found; supervisor remains stopped");
      return;
    };

    let runtime_root = std::env::var_os("FLOWORD_DONUT_RESOURCE_ROOT").map(PathBuf::from).or_else(|| executable.parent().map(Path::to_path_buf));
    let Some(runtime_root) = runtime_root else {
      self.set_failure(RuntimeSupervisorState::Degraded, "cannot resolve Donut runtime resource root");
      return;
    };

    let log_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("Floword")).join("logs");
    if let Err(error) = std::fs::create_dir_all(&log_dir) {
      self.set_failure(RuntimeSupervisorState::Degraded, format!("cannot create runtime log directory: {error}"));
      return;
    }
    let log_file = match OpenOptions::new().create(true).append(true).open(log_dir.join("donut-runtime.log")) {
      Ok(file) => file,
      Err(error) => {
        self.set_failure(RuntimeSupervisorState::Degraded, format!("cannot open runtime log: {error}"));
        return;
      },
    };

    let stdout_log = match log_file.try_clone() {
      Ok(file) => file,
      Err(error) => {
        self.set_failure(RuntimeSupervisorState::Degraded, format!("cannot clone runtime log handle: {error}"));
        return;
      },
    };
    let mut command = background_command(Command::new(&executable));
    command.current_dir(&runtime_root).stdin(Stdio::null()).stdout(Stdio::from(stdout_log)).stderr(Stdio::from(log_file)).env("FLOWORD_DONUT_HOST", HOST).env("FLOWORD_DONUT_PORT", PORT.to_string()).env("FLOWORD_PARENT_PID", std::process::id().to_string()).env("FLOWORD_DONUT_RESOURCE_ROOT", &runtime_root);
    if let Some(data_dir) = resolve_shared_donut_data_dir() {
      command.env("DONUTBROWSER_DATA_DIR", data_dir);
    }

    let child = match command.spawn() {
      Ok(child) => child,
      Err(error) => {
        self.set_failure(RuntimeSupervisorState::Degraded, format!("failed to start {}: {error}", executable.display()));
        return;
      },
    };
    let pid = child.id();
    if let Ok(mut guard) = self.child.lock() {
      *guard = Some(child);
    }
    if let Ok(mut status) = self.status.lock() {
      status.state = RuntimeSupervisorState::Starting;
      status.pid = Some(pid);
      status.last_error = None;
      status.started_at = Some(chrono::Utc::now().to_rfc3339());
    }
    info!("Started Floword Donut runtime {} (pid={pid})", executable.display());

    let supervisor = self.clone();
    std::thread::spawn(move || supervisor.wait_until_ready());
  }

  fn wait_until_ready(&self) {
    let deadline = Instant::now() + READY_DEADLINE;
    loop {
      if let Some(health) = runtime_health() {
        if health.protocol.as_deref() == Some("floword-production") && health.protocol_version == Some(1) {
          if let Ok(mut status) = self.status.lock() {
            status.state = RuntimeSupervisorState::Ready;
            status.pid = health.pid.or(status.pid);
            status.last_health_at = Some(chrono::Utc::now().to_rfc3339());
          }
          info!("Floword Donut runtime ({}) is READY on http://{HOST}:{PORT}", health.runtime);

          self.monitor_child_exit();
          return;
        }
        self.set_failure(RuntimeSupervisorState::PortConflict, format!("127.0.0.1:{PORT} is served by incompatible runtime {}", health.runtime));
        self.stop_child_only();
        return;
      }

      if let Ok(mut guard) = self.child.lock() {
        if let Some(child) = guard.as_mut() {
          match child.try_wait() {
            Ok(Some(status)) => {
              guard.take();
              self.set_failure(RuntimeSupervisorState::Crashed, format!("runtime exited before readiness: {status}"));
              return;
            },
            Ok(None) => {},
            Err(error) => warn!("runtime child status check failed: {error}"),
          }
        }
      }
      if Instant::now() >= deadline {
        self.set_failure(RuntimeSupervisorState::Degraded, format!("runtime did not become ready within {}s", READY_DEADLINE.as_secs()));
        self.stop_child_only();
        return;
      }
      std::thread::sleep(POLL_INTERVAL);
    }
  }

  fn monitor_child_exit(&self) {
    loop {
      std::thread::sleep(Duration::from_secs(1));
      let exited = self.child.lock().ok().and_then(|mut guard| guard.as_mut().map(Child::try_wait));
      match exited {
        Some(Ok(Some(status))) => {
          if let Ok(mut guard) = self.child.lock() {
            guard.take();
          }
          self.set_failure(RuntimeSupervisorState::Crashed, format!("runtime exited after readiness: {status}"));
          return;
        },
        Some(Ok(None)) => {},
        Some(Err(error)) => warn!("runtime health monitor failed: {error}"),
        None => return,
      }
    }
  }

  pub fn stop(&self) {
    if let Ok(mut status) = self.status.lock() {
      status.state = RuntimeSupervisorState::Stopping;
    }
    self.stop_child_only();
    if let Ok(mut status) = self.status.lock() {
      status.state = RuntimeSupervisorState::Stopped;
      status.pid = None;
      status.last_health_at = None;
    }
  }

  fn stop_child_only(&self) {
    if let Ok(mut guard) = self.child.lock() {
      if let Some(mut child) = guard.take() {
        terminate_child_tree(&mut child);
      }
    }
  }

  fn set_failure(&self, state: RuntimeSupervisorState, message: impl Into<String>) {
    let message = message.into();
    warn!("Floword Donut runtime {:?}: {message}", state);
    if let Ok(mut status) = self.status.lock() {
      status.state = state;
      status.last_error = Some(message);
    }
  }
}

pub fn start_runtime_supervisor(app: &AppHandle) {
  if let Some(supervisor) = app.try_state::<RuntimeSupervisor>() {
    supervisor.start(app);
  }
}

/// Starts the dev Playwright owner before Donut. Packaged builds may provide
/// an explicit FLOWORD_PLAYWRIGHT_SIDECAR entrypoint; no duplicate is spawned
/// when an authenticated Floword runtime already owns port 9223.
pub fn start_playwright_runtime(app: &AppHandle) {
  if let Some(slot) = PLAYWRIGHT_CHILD.get() {
    if let Ok(mut guard) = slot.lock() {
      if let Some(child) = guard.as_mut() {
        match child.try_wait() {
          Ok(None) => return,
          Ok(Some(_)) | Err(_) => {
            guard.take();
          },
        }
      }
    }
  }
  if std::env::var_os("FLOWORD_PLAYWRIGHT_RUNTIME_URL").is_none() {
    std::env::set_var("FLOWORD_PLAYWRIGHT_RUNTIME_URL", format!("http://{HOST}:{PLAYWRIGHT_PORT}"));
  }
  if playwright_health_ok() {
    return;
  }
  if port_open_at(PLAYWRIGHT_PORT) {
    warn!("PORT_CONFLICT: 127.0.0.1:{PLAYWRIGHT_PORT} is occupied by an incompatible process");
    return;
  }
  let resource_dir = app.path().resource_dir().ok();
  if !runtime_manifest_ready(app) {
    return;
  }
  let sidecar = std::env::var_os("FLOWORD_PLAYWRIGHT_SIDECAR").map(PathBuf::from).or_else(|| resource_dir.as_ref().map(|root| root.join("playwright-sidecar/src/server.js")).filter(|path| path.is_file())).or_else(|| {
    let cwd = std::env::current_dir().ok()?;
    [cwd.join("tools/playwright-sidecar/src/server.js"), cwd.join("resources/playwright-sidecar/server.js"), cwd.join("..\\..\\..\\tools\\playwright-sidecar\\src\\server.js")].into_iter().find(|path| path.is_file())
  });
  let Some(sidecar) = sidecar.filter(|path| path.is_file()) else {
    warn!("Playwright sidecar entrypoint not found; Floword will report PLAYWRIGHT_RUNTIME_OFFLINE");
    return;
  };
  let node = resolve_playwright_node(app);
  let log_dir = std::env::var_os("FLOWORD_LOG_DIR").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("Floword"));
  if let Err(error) = std::fs::create_dir_all(&log_dir) {
    warn!("failed to create Playwright log directory: {error}");
    return;
  }
  let stdout = match OpenOptions::new().create(true).append(true).open(log_dir.join("playwright-sidecar.log")) {
    Ok(file) => file,
    Err(error) => {
      warn!("failed to open Playwright log: {error}");
      return;
    },
  };
  let stderr = match stdout.try_clone() {
    Ok(file) => file,
    Err(error) => {
      warn!("failed to clone Playwright log: {error}");
      return;
    },
  };
  let mut command = background_command(Command::new(node));
  command.arg(&sidecar).current_dir(sidecar.parent().unwrap_or(Path::new("."))).env("PLAYWRIGHT_SIDECAR_PORT", PLAYWRIGHT_PORT.to_string()).env("FLOWORD_PARENT_PID", std::process::id().to_string()).stdin(Stdio::null()).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
  if let Some(root) = resource_dir.as_ref() {
    if root.join("playwright").is_dir() {
      command.env("PLAYWRIGHT_BROWSERS_PATH", root.join("playwright"));
    }
  }
  if std::env::var_os("FLOWORD_CHROMEX_EXTENSION_PATH").is_none() {
    if let Some(path) = resource_dir.as_ref().map(|root| root.join("chromex-extension")).filter(|path| path.join("manifest.json").is_file()) {
      command.env("FLOWORD_CHROMEX_EXTENSION_PATH", path);
    }
  }
  if std::env::var_os("FLOWORD_CHROMEX_EXTENSION_PATH").is_none() {
    if let Some(path) = std::env::current_dir().ok().and_then(|cwd| [cwd.join("resources/chromex-extension"), cwd.join("..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod"), cwd.join("..\\..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod"), cwd.join("..\\..\\..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod")].into_iter().find(|path| path.join("manifest.json").is_file())) {
      command.env("FLOWORD_CHROMEX_EXTENSION_PATH", path);
    }
  }
  if let Some(browsers) = std::env::var_os("FLOWORD_PLAYWRIGHT_BROWSERS_PATH") {
    command.env("PLAYWRIGHT_BROWSERS_PATH", browsers);
  }
  let child = match command.spawn() {
    Ok(child) => child,
    Err(error) => {
      warn!("Failed to start Playwright runtime: {error}");
      return;
    },
  };
  let pid = child.id();
  let slot = PLAYWRIGHT_CHILD.get_or_init(|| Arc::new(Mutex::new(None))).clone();
  if let Ok(mut guard) = slot.lock() {
    *guard = Some(child);
  }
  info!("Started Floword Playwright runtime (pid={pid})");
  std::thread::spawn(move || loop {
    // Never hold the global child mutex while waiting. Shutdown must be able
    // to acquire it and terminate the process tree immediately.
    let result = slot.lock().ok().and_then(|mut guard| guard.as_mut().map(Child::try_wait));
    match result {
      Some(Ok(Some(status))) => {
        warn!("Playwright sidecar exited (pid={pid}): {status}");
        if let Ok(mut guard) = slot.lock() {
          guard.take();
        }
        break;
      },
      Some(Ok(None)) => std::thread::sleep(Duration::from_millis(250)),
      Some(Err(error)) => {
        warn!("Playwright sidecar monitor failed (pid={pid}): {error}");
        if let Ok(mut guard) = slot.lock() {
          guard.take();
        }
        break;
      },
      None => break,
    }
  });
}

pub fn stop_playwright_runtime() {
  if let Some(slot) = PLAYWRIGHT_CHILD.get() {
    if let Ok(mut guard) = slot.lock() {
      if let Some(mut child) = guard.take() {
        terminate_child_tree(&mut child);
      }
    }
  }
}

fn resolve_playwright_node(app: &AppHandle) -> PathBuf {
  if let Some(path) = std::env::var_os("FLOWORD_PLAYWRIGHT_NODE") {
    return PathBuf::from(path);
  }
  if let Ok(resources) = app.path().resource_dir() {
    for candidate in [resources.join("node/node.exe"), resources.join("node.exe")] {
      if candidate.is_file() {
        return candidate;
      }
    }
  }
  if let Ok(exe) = std::env::current_exe() {
    if let Some(dir) = exe.parent() {
      for candidate in [dir.join("resources/node/node.exe"), dir.join("node/node.exe")] {
        if candidate.is_file() {
          return candidate;
        }
      }
    }
  }
  #[cfg(debug_assertions)]
  {
    PathBuf::from("node")
  }
  #[cfg(not(debug_assertions))]
  {
    PathBuf::from("node.exe")
  }
}

fn verify_runtime_manifest(root: &Path) -> Result<(), String> {
  let path = root.join("runtime-manifest.sha256.json");
  let raw = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
  let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| format!("invalid manifest JSON: {e}"))?;
  if value.get("schemaVersion").and_then(|v| v.as_u64()) != Some(1) || value.get("generatedAt").and_then(|v| v.as_str()).is_none() || value.get("files").and_then(|v| v.as_object()).is_none() {
    return Err("manifest has no generatedAt/files".to_string());
  }
  let files = value.get("files").and_then(|v| v.as_object()).unwrap();
  if files.is_empty() {
    return Err("manifest contains no files".to_string());
  }
  let required = value.get("requiredArtifacts").and_then(|v| v.as_array()).ok_or_else(|| "manifest has no requiredArtifacts".to_string())?;
  let required_names = required.iter().filter_map(serde_json::Value::as_str).collect::<std::collections::HashSet<_>>();
  for name in REQUIRED_RUNTIME_ARTIFACTS {
    if !required_names.contains(name) {
      return Err(format!("manifest requiredArtifacts omits fixed artifact: {name}"));
    }
    if !files.contains_key(*name) {
      return Err(format!("required artifact missing from manifest: {name}"));
    }
  }
  for artifact in required {
    let name = artifact.as_str().ok_or_else(|| "invalid required artifact entry".to_string())?;
    if !files.contains_key(name) {
      return Err(format!("required artifact missing from manifest: {name}"));
    }
    if name.ends_with(".gitkeep") || name.contains("placeholder") {
      return Err(format!("placeholder artifact is not allowed: {name}"));
    }
  }
  for (relative, expected) in files {
    let expected = expected.as_str().ok_or_else(|| format!("invalid hash for {relative}"))?;
    let file = root.join(relative);
    let bytes = std::fs::read(&file).map_err(|e| format!("missing {relative}: {e}"))?;
    let actual: String = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    if actual != expected {
      return Err(format!("SHA256 mismatch for {relative}"));
    }
  }
  Ok(())
}

/// Release startup is fail-closed. Debug source runs may opt in explicitly
/// when no staged binaries exist; this flag is never honored in release.
pub fn runtime_manifest_ready(app: &AppHandle) -> bool {
  let Some(root) = app.path().resource_dir().ok() else {
    return false;
  };
  if cfg!(debug_assertions) && std::env::var_os("FLOWORD_ALLOW_SOURCE_RUNTIME").is_some() {
    return true;
  }
  match verify_runtime_manifest(&root) {
    Ok(()) => true,
    Err(error) => {
      warn!("RUNTIME_INTEGRITY_ERROR: {error}");
      false
    },
  }
}

fn playwright_health_ok() -> bool {
  let Ok(mut stream) = TcpStream::connect_timeout(&format!("{HOST}:{PLAYWRIGHT_PORT}").parse().unwrap(), Duration::from_millis(250)) else {
    return false;
  };
  let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
  let authorization = std::env::var("FLOWORD_SIDECAR_TOKEN").map(|token| format!("Authorization: Bearer {token}\r\n")).unwrap_or_default();
  let _ = stream.write_all(format!("GET /health HTTP/1.1\r\nHost: {HOST}:{PLAYWRIGHT_PORT}\r\n{authorization}Connection: close\r\n\r\n").as_bytes());
  let mut response = String::new();
  let _ = stream.read_to_string(&mut response);
  let Some(body) = response.split("\r\n\r\n").nth(1) else {
    return false;
  };
  let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
    return false;
  };
  value.get("service").and_then(serde_json::Value::as_str) == Some("floword-playwright-runtime") && value.get("protocol").and_then(serde_json::Value::as_str) == Some("floword-playwright") && value.get("protocolVersion").and_then(serde_json::Value::as_u64) == Some(1)
}

#[tauri::command]
pub fn get_donut_runtime_status(supervisor: tauri::State<'_, RuntimeSupervisor>) -> RuntimeSupervisorStatus {
  supervisor.status()
}

fn resolve_runtime_executable(app: &AppHandle) -> Option<PathBuf> {
  let mut candidates = Vec::new();
  if let Some(path) = std::env::var_os("FLOWORD_DONUT_RUNTIME_EXE") {
    candidates.push(PathBuf::from(path));
  }

  if let Ok(exe) = std::env::current_exe() {
    if let Some(dir) = exe.parent() {
      candidates.push(dir.join("floword-donut-runtime.exe"));
      candidates.push(dir.join("resources/donut-runtime/floword-donut-runtime.exe"));
    }
  }
  if let Ok(resources) = app.path().resource_dir() {
    candidates.push(resources.join("donut-runtime/floword-donut-runtime.exe"));
    candidates.push(resources.join("floword-donut-runtime.exe"));
  }
  candidates.into_iter().find(|path| path.is_file())
}

/// Keep the embedded runtime on the same profile catalog as Donut Manager.
/// Production can provide an explicit root; debug runs prefer the packaged
/// Manager's `DonutBrowser` catalog and fall back to `DonutBrowserDev`.
fn resolve_shared_donut_data_dir() -> Option<PathBuf> {
  if let Some(path) = std::env::var_os("FLOWORD_DONUT_DATA_DIR") {
    if !path.is_empty() {
      return Some(PathBuf::from(path));
    }
  }

  #[cfg(debug_assertions)]
  {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
      let local = PathBuf::from(local_app_data);
      let dev = local.join("DonutBrowserDev");
      if dev.join("profiles").is_dir() {
        return Some(dev);
      }
      return Some(local.join("DonutBrowser"));
    }
    return None;
  }

  #[cfg(not(debug_assertions))]
  None
}

fn port_open() -> bool {
  port_open_at(PORT)
}

fn port_open_at(port: u16) -> bool {
  format!("{HOST}:{port}").to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).and_then(|address| TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()).is_some()
}

fn runtime_health() -> Option<RuntimeHealth> {
  let mut stream = TcpStream::connect_timeout(&format!("{HOST}:{PORT}").parse().ok()?, Duration::from_millis(300)).ok()?;
  stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
  let request = format!("GET /v1/runtime/health HTTP/1.1\r\nHost: {HOST}:{PORT}\r\nConnection: close\r\n\r\n");
  stream.write_all(request.as_bytes()).ok()?;
  let mut response = String::new();
  stream.read_to_string(&mut response).ok()?;
  if !(response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200")) {
    return None;
  }
  let body = response.split("\r\n\r\n").nth(1)?;
  let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
  Some(RuntimeHealth { runtime: value.get("runtime")?.as_str()?.to_string(), pid: value.get("pid").and_then(serde_json::Value::as_u64).map(|pid| pid as u32), protocol: value.get("protocol").and_then(serde_json::Value::as_str).map(str::to_string), protocol_version: value.get("protocolVersion").and_then(serde_json::Value::as_u64).map(|version| version as u32) })
}

fn terminate_child_tree(child: &mut Child) {
  let pid = child.id();
  #[cfg(target_os = "windows")]
  {
    let _ = background_command(Command::new("taskkill")).args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
  }
  let _ = child.kill();
  let _ = child.wait();
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::BTreeMap;

  fn fixture_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("floword-runtime-manifest-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
  }

  fn write_manifest(root: &Path, include: &[&str]) {
    let mut files = BTreeMap::new();
    for relative in include {
      let path = root.join(relative);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(&path, format!("fixture:{relative}")).unwrap();
      let digest: String = Sha256::digest(std::fs::read(&path).unwrap()).iter().map(|byte| format!("{byte:02x}")).collect();
      files.insert((*relative).to_string(), digest);
    }
    let required: Vec<&str> = REQUIRED_RUNTIME_ARTIFACTS.to_vec();
    let manifest = serde_json::json!({ "schemaVersion": 1, "generatedAt": "2026-01-01T00:00:00Z", "requiredArtifacts": required, "files": files });
    std::fs::write(root.join("runtime-manifest.sha256.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
  }

  #[test]
  fn manifest_requires_fixed_donut_artifact() {
    let root = fixture_root("missing-donut");
    let mut include = REQUIRED_RUNTIME_ARTIFACTS.to_vec();
    include.retain(|path| !path.starts_with("donut-runtime/"));
    include.push("playwright/chromium/chrome.exe");
    write_manifest(&root, &include);
    assert!(verify_runtime_manifest(&root).is_err());
    let _ = std::fs::remove_dir_all(root);
  }

  #[test]
  fn complete_manifest_without_chromium_passes() {
    let root = fixture_root("complete");
    write_manifest(&root, REQUIRED_RUNTIME_ARTIFACTS);
    assert!(verify_runtime_manifest(&root).is_ok());
    let _ = std::fs::remove_dir_all(root);
  }
}
