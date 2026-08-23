use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{info, warn};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::background_command::background_command;

const HOST: &str = "127.0.0.1";
const PORT: u16 = 10108;
const READY_DEADLINE: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(400);
const PLAYWRIGHT_PORT: u16 = 9223;

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
pub fn start_playwright_runtime() {
  if std::env::var_os("FLOWORD_PLAYWRIGHT_RUNTIME_URL").is_none() {
    std::env::set_var("FLOWORD_PLAYWRIGHT_RUNTIME_URL", format!("http://{HOST}:{PLAYWRIGHT_PORT}"));
  }
  if playwright_health_ok() { return; }
  let sidecar = std::env::var_os("FLOWORD_PLAYWRIGHT_SIDECAR").map(PathBuf::from).or_else(|| std::env::current_dir().ok().and_then(|cwd| [cwd.join("tools/playwright-sidecar/src/server.js"), cwd.join("..\\..\\..\\tools\\playwright-sidecar\\src\\server.js")].into_iter().find(|path| path.is_file())));
  let Some(sidecar) = sidecar.filter(|path| path.is_file()) else { warn!("Playwright sidecar entrypoint not found; Floword will report PLAYWRIGHT_RUNTIME_OFFLINE"); return; };
  let mut command = background_command(Command::new("node"));
  command.arg(&sidecar).current_dir(sidecar.parent().unwrap_or(Path::new("."))).env("PLAYWRIGHT_SIDECAR_PORT", PLAYWRIGHT_PORT.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
  if std::env::var_os("FLOWORD_CHROMEX_EXTENSION_PATH").is_none() {
    if let Some(path) = std::env::current_dir().ok().and_then(|cwd| [cwd.join("..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod"), cwd.join("..\\..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod"), cwd.join("..\\..\\..\\..\\chromex\\packages\\extension\\build\\chrome-mv3-prod")].into_iter().find(|path| path.join("manifest.json").is_file())) { command.env("FLOWORD_CHROMEX_EXTENSION_PATH", path); }
  }
  match command.spawn() { Ok(child) => info!("Started Floword Playwright runtime (pid={})", child.id()), Err(error) => warn!("Failed to start Playwright runtime: {error}") }
}

fn playwright_health_ok() -> bool {
  let Ok(mut stream) = TcpStream::connect_timeout(&format!("{HOST}:{PLAYWRIGHT_PORT}").parse().unwrap(), Duration::from_millis(250)) else { return false; };
  let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
  let _ = stream.write_all(format!("GET /health HTTP/1.1\r\nHost: {HOST}:{PLAYWRIGHT_PORT}\r\nConnection: close\r\n\r\n").as_bytes());
  let mut response = String::new(); let _ = stream.read_to_string(&mut response);
  response.contains("floword-playwright-runtime") && response.contains("\"protocolVersion\":1")
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
  format!("{HOST}:{PORT}").to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).and_then(|address| TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()).is_some()
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
