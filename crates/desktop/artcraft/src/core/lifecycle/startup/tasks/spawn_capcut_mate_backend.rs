//! Start capcut-mate (Python BE) as a child process of the Tauri app.
//!
//! **When it auto-starts**
//! - **Packaged Tauri** (sidecar or `capcut-mate/` next to exe / in resources): yes
//! - **Dev / repo checkout**: **no** — run BE yourself:
//!   `cd capcut-mate` then `uv run main.py`
//! - Force dev auto-start: `CAPCUT_MATE_AUTO_START=1` (+ optional `CAPCUT_MATE_DIR`)
//! - Force off everywhere: `CAPCUT_MATE_AUTO_START=0`
//!
//! Lookup order when starting:
//! 1. Sidecar `capcut-mate-server.exe` (resources / next to app exe)
//! 2. Bundled folder `capcut-mate/` (resources / next to app exe)
//! 3. Env `CAPCUT_MATE_DIR` or repo layouts (dev, only if AUTO_START=1)

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Manager};

use super::background_command::background_command;

const DEFAULT_PORT: u16 = 30000;
const HEALTH_PATH: &str = "/health";
const SIDECAR_NAME: &str = "capcut-mate-server.exe";
const MATE_DIR_NAME: &str = "capcut-mate";

const READY_EVENT: &str = "backend://ready";
const ERROR_EVENT: &str = "backend://error";
/// Total time to wait for the BE to start LISTENing before giving up.
const READY_DEADLINE: Duration = Duration::from_secs(30);
/// Gap between port probes while waiting for the BE to come up.
const READY_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Managed Tauri state — killed when app process exits (Drop).
pub struct CapcutMateProcess {
  child: Mutex<Option<Child>>,
}

impl Drop for CapcutMateProcess {
  fn drop(&mut self) {
    self.stop();
  }
}

impl CapcutMateProcess {
  pub fn stop(&self) {
    if let Ok(mut guard) = self.child.lock() {
      if let Some(mut child) = guard.take() {
        terminate_child_tree(&mut child);
      }
    }
  }
}

fn terminate_child_tree(child: &mut Child) {
  let pid = child.id();
  info!("Stopping embedded capcut-mate (pid={pid})");
  #[cfg(target_os = "windows")]
  {
    let _ = background_command(Command::new("taskkill")).args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
  }
  let _ = child.kill();
  let _ = child.wait();
}

fn port_open(port: u16) -> bool {
  let addr: std::net::SocketAddr = match format!("127.0.0.1:{port}").parse() {
    Ok(a) => a,
    Err(_) => return false,
  };
  TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

pub(crate) fn health_ready(port: u16) -> bool {
  let addr: std::net::SocketAddr = match format!("127.0.0.1:{port}").parse() {
    Ok(addr) => addr,
    Err(_) => return false,
  };
  let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
    return false;
  };
  let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
  let request = format!("GET {HEALTH_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
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

fn emit_backend_ready(app: &AppHandle, port: u16) {
  info!("capcut-mate ready on :{port} — emitting {READY_EVENT}");
  if let Err(e) = app.emit(READY_EVENT, serde_json::json!({ "port": port })) {
    warn!("Failed to emit {READY_EVENT}: {e}");
  }
}

fn emit_backend_error(app: &AppHandle, message: impl Into<String>) {
  let message = message.into();
  error!("capcut-mate error — emitting {ERROR_EVENT}: {message}");
  if let Err(e) = app.emit(ERROR_EVENT, serde_json::json!({ "message": message })) {
    warn!("Failed to emit {ERROR_EVENT}: {e}");
  }
}

/// Poll the port until it LISTENs (emit ready) or the deadline passes (emit error).
/// Safe to block here — this runs on the background startup thread, not the UI thread.
fn wait_for_backend_ready(app: &AppHandle, process: &CapcutMateProcess, port: u16) {
  let deadline = std::time::Instant::now() + READY_DEADLINE;
  let mut attempt = 0_u32;
  loop {
    attempt += 1;
    if health_ready(port) {
      info!("[backend][unified] READY http://127.0.0.1:{port}{HEALTH_PATH}");
      emit_backend_ready(app, port);
      return;
    }
    match process.child.lock() {
      Ok(mut guard) => match guard.as_mut().map(Child::try_wait) {
        Some(Ok(Some(status))) => {
          guard.take();
          emit_backend_error(app, format!("[backend][unified] exited before readiness: {status}"));
          return;
        },
        Some(Err(error)) => warn!("[backend][unified] process status failed: {error}"),
        Some(Ok(None)) => {},
        None => {
          emit_backend_error(app, "[backend][unified] process ownership disappeared during readiness");
          return;
        },
      },
      Err(error) => {
        emit_backend_error(app, format!("[backend][unified] process lock poisoned: {error}"));
        return;
      },
    }
    if std::time::Instant::now() >= deadline {
      emit_backend_error(app, format!("capcut-mate did not start listening on :{port} within {}s", READY_DEADLINE.as_secs()));
      if let Ok(mut guard) = process.child.lock() {
        if let Some(mut child) = guard.take() {
          terminate_child_tree(&mut child);
        }
      }
      return;
    }
    if attempt == 1 || attempt % 10 == 0 {
      info!("[backend][unified] readiness attempt {attempt}: http://127.0.0.1:{port}{HEALTH_PATH}");
    }
    std::thread::sleep(READY_POLL_INTERVAL);
  }
}

fn is_mate_dir(p: &Path) -> bool {
  p.is_dir() && p.join("main.py").is_file()
}

fn exe_dir() -> Option<PathBuf> {
  std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn resource_dir(app: &AppHandle) -> Option<PathBuf> {
  app.path().resource_dir().ok()
}

fn push_if(paths: &mut Vec<PathBuf>, p: PathBuf) {
  if !paths.iter().any(|x| x == &p) {
    paths.push(p);
  }
}

/// Prefer frozen sidecar next to app / in resources.
fn resolve_sidecar(app: &AppHandle) -> Option<PathBuf> {
  let mut candidates: Vec<PathBuf> = Vec::new();

  if let Ok(p) = std::env::var("CAPCUT_MATE_SIDECAR") {
    push_if(&mut candidates, PathBuf::from(p));
  }
  if let Some(dir) = exe_dir() {
    push_if(&mut candidates, dir.join(SIDECAR_NAME));
    // NSIS installs Tauri resources below <install-dir>/resources, while
    // app.path().resource_dir() can still report <install-dir> on Windows.
    push_if(&mut candidates, dir.join("resources").join(SIDECAR_NAME));
    push_if(&mut candidates, dir.join(MATE_DIR_NAME).join(SIDECAR_NAME));
  }
  if let Some(res) = resource_dir(app) {
    push_if(&mut candidates, res.join(SIDECAR_NAME));
    push_if(&mut candidates, res.join("resources").join(SIDECAR_NAME));
    push_if(&mut candidates, res.join(MATE_DIR_NAME).join(SIDECAR_NAME));
  }

  for c in candidates {
    if c.is_file() {
      return Some(c);
    }
  }
  None
}

fn resolve_mate_dir(app: &AppHandle) -> Option<PathBuf> {
  if let Ok(p) = std::env::var("CAPCUT_MATE_DIR") {
    let pb = PathBuf::from(p);
    if is_mate_dir(&pb) {
      return Some(pb);
    }
    warn!("CAPCUT_MATE_DIR is not a valid mate dir: {}", pb.display());
  }

  let mut candidates: Vec<PathBuf> = Vec::new();

  // Bundled with installed / portable app
  if let Some(dir) = exe_dir() {
    push_if(&mut candidates, dir.join(MATE_DIR_NAME));
    push_if(&mut candidates, dir.join("resources").join(MATE_DIR_NAME));
    if let Some(parent) = dir.parent() {
      push_if(&mut candidates, parent.join(MATE_DIR_NAME));
    }
  }
  if let Some(res) = resource_dir(app) {
    push_if(&mut candidates, res.join(MATE_DIR_NAME));
    push_if(&mut candidates, res.clone());
  }

  // Repo / compile-time layout (dev + build machine)
  let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  push_if(&mut candidates, manifest.join("../../../capcut-mate"));
  push_if(&mut candidates, manifest.join("../../../../capcut-mate"));
  push_if(&mut candidates, manifest.join(MATE_DIR_NAME));
  push_if(&mut candidates, manifest.join("resources").join(MATE_DIR_NAME));

  if let Ok(cwd) = std::env::current_dir() {
    push_if(&mut candidates, cwd.join(MATE_DIR_NAME));
    push_if(&mut candidates, cwd.join("artcraft").join(MATE_DIR_NAME));
  }

  for c in candidates {
    if let Ok(c) = c.canonicalize() {
      if is_mate_dir(&c) {
        return Some(c);
      }
    } else if is_mate_dir(&c) {
      return Some(c);
    }
  }
  None
}

fn spawn_sidecar(exe: &Path) -> Result<Child, String> {
  let cwd = exe.parent().unwrap_or_else(|| Path::new("."));
  let log_dir = std::env::temp_dir().join("artcraft-unified");
  std::fs::create_dir_all(&log_dir).map_err(|error| format!("Cannot create unified log directory: {error}"))?;
  let stdout_path = log_dir.join("unified.stdout.log");
  let stderr_path = log_dir.join("unified.stderr.log");
  let stdout = OpenOptions::new().create(true).append(true).open(&stdout_path).map_err(|error| format!("Cannot open {}: {error}", stdout_path.display()))?;
  let stderr = OpenOptions::new().create(true).append(true).open(&stderr_path).map_err(|error| format!("Cannot open {}: {error}", stderr_path.display()))?;

  info!("[backend][unified] executable={}", exe.display());
  info!("[backend][unified] args=[] cwd={}", cwd.display());
  info!("[backend][unified] stdout={} stderr={}", stdout_path.display(), stderr_path.display());
  background_command(Command::new(exe)).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).spawn().map_err(|e| format!("Failed to spawn sidecar {}: {e}", exe.display()))
}

fn spawn_mate_from_dir(dir: &Path) -> Result<Child, String> {
  // Portable venv shipped next to source (build may copy .venv)
  let venv_py = dir.join(".venv").join("Scripts").join("python.exe");
  if venv_py.is_file() {
    match background_command(Command::new(&venv_py)).arg("main.py").current_dir(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
      Ok(c) => return Ok(c),
      Err(e) => warn!("venv python failed: {e}"),
    }
  }

  // Prefer uv (project uses uv run main.py)
  if let Ok(child) = background_command(Command::new("uv")).args(["run", "main.py"]).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
    return Ok(child);
  }

  // Fallback: system python
  background_command(Command::new("python")).arg("main.py").current_dir(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().or_else(|_| background_command(Command::new("python3")).arg("main.py").current_dir(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()).map_err(|e| format!("Failed to spawn capcut-mate from {}: {e}", dir.display()))
}

fn env_auto_start() -> Option<bool> {
  match std::env::var("CAPCUT_MATE_AUTO_START") {
    Ok(v) => {
      let v = v.to_ascii_lowercase();
      match v.as_str() {
        "0" | "false" | "no" | "off" => Some(false),
        "1" | "true" | "yes" | "on" => Some(true),
        _ => None,
      }
    },
    Err(_) => None,
  }
}

/// True if `dir` is next to the installed app / Tauri resources (not monorepo source).
fn is_packaged_mate_dir(app: &AppHandle, dir: &Path) -> bool {
  let dir_canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());

  let mut roots: Vec<PathBuf> = Vec::new();
  if let Some(exe) = exe_dir() {
    push_if(&mut roots, exe.clone());
    push_if(&mut roots, exe.join("resources"));
    if let Some(parent) = exe.parent() {
      push_if(&mut roots, parent.to_path_buf());
    }
  }
  if let Some(res) = resource_dir(app) {
    push_if(&mut roots, res);
  }

  for root in roots {
    let root_canon = root.canonicalize().unwrap_or(root);
    if dir_canon.starts_with(&root_canon) {
      return true;
    }
  }
  false
}

fn manage_empty(app: &AppHandle) {
  app.manage(CapcutMateProcess { child: Mutex::new(None) });
}

/// Call from Tauri setup. Never fails startup of the whole app.
pub fn spawn_capcut_mate_backend(app: &AppHandle) {
  info!("[production-paths][unified] exe_dir={:?} resource_dir={:?} cwd={:?}", exe_dir(), resource_dir(app), std::env::current_dir().ok());
  // Explicit off
  if env_auto_start() == Some(false) {
    info!("CAPCUT_MATE_AUTO_START=0 — BE not started (run capcut-mate manually if needed)");
    manage_empty(app);
    return;
  }

  if port_open(DEFAULT_PORT) {
    if !health_ready(DEFAULT_PORT) {
      emit_backend_error(app, format!("[backend][unified] PORT_IN_USE: :{DEFAULT_PORT} is occupied but {HEALTH_PATH} is not healthy"));
      manage_empty(app);
      return;
    }
    info!("capcut-mate already listening on :{DEFAULT_PORT} — reuse (no spawn)");
    manage_empty(app);
    emit_backend_ready(app, DEFAULT_PORT);
    return;
  }

  let force_dev = env_auto_start() == Some(true);

  // 1) Frozen sidecar — only present on packaged builds (or CAPCUT_MATE_SIDECAR)
  if let Some(sidecar) = resolve_sidecar(app) {
    match spawn_sidecar(&sidecar) {
      Ok(child) => {
        info!("Started capcut-mate sidecar {} (pid={}) [packaged]", sidecar.display(), child.id());
        app.manage(CapcutMateProcess { child: Mutex::new(Some(child)) });
        wait_for_backend_ready(app, app.state::<CapcutMateProcess>().inner(), DEFAULT_PORT);
        return;
      },
      Err(e) => warn!("{e}"),
    }
  }

  // 2) Folder + uv|python|.venv
  let Some(dir) = resolve_mate_dir(app) else {
    if force_dev {
      warn!("CAPCUT_MATE_AUTO_START=1 but capcut-mate not found. Set CAPCUT_MATE_DIR or run: cd capcut-mate; uv run main.py");
    } else {
      info!("Dev mode: capcut-mate BE not auto-started. For CapCut Automation run: cd capcut-mate; uv run main.py (or build packaged app)");
    }
    manage_empty(app);
    return;
  };

  // Packaged layout always starts; repo/source only with CAPCUT_MATE_AUTO_START=1
  if !force_dev && !is_packaged_mate_dir(app, &dir) {
    info!("Dev mode: found source capcut-mate at {} but not auto-starting. Run: uv run main.py there (or set CAPCUT_MATE_AUTO_START=1). Packaged .exe will auto-start BE.", dir.display());
    manage_empty(app);
    return;
  }

  match spawn_mate_from_dir(&dir) {
    Ok(child) => {
      info!("Started embedded capcut-mate from {} (pid={})", dir.display(), child.id());
      app.manage(CapcutMateProcess { child: Mutex::new(Some(child)) });
      wait_for_backend_ready(app, app.state::<CapcutMateProcess>().inner(), DEFAULT_PORT);
    },
    Err(e) => {
      warn!("{e}");
      emit_backend_error(app, e);
      manage_empty(app);
    },
  }
}
