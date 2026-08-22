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
      self.set_failure(RuntimeSupervisorState::PortConflict, "127.0.0.1:10108 is already occupied; refusing to adopt or kill the process");
      return;
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

    // Also spawn Donut Browser Desktop App Window (Tauri App)
    let _ = background_command(Command::new("cmd"))
      .args(["/c", "cd /d D:\\capcutpolot\\donutbrowser && pnpm tauri dev"])
      .spawn();

    let supervisor = self.clone();
    std::thread::spawn(move || supervisor.wait_until_ready());
  }

  fn wait_until_ready(&self) {
    let deadline = Instant::now() + READY_DEADLINE;
    loop {
      if let Some(runtime) = runtime_health() {
        if runtime == "floword-donut-runtime" || runtime == "donutbrowser" || runtime == "donut" || !runtime.is_empty() {
          if let Ok(mut status) = self.status.lock() {
            status.state = RuntimeSupervisorState::Ready;
            status.last_health_at = Some(chrono::Utc::now().to_rfc3339());
          }
          info!("Floword Donut runtime ({runtime}) is READY on http://{HOST}:{PORT}");

          self.monitor_child_exit();
          return;
        }
        self.set_failure(RuntimeSupervisorState::PortConflict, format!("127.0.0.1:{PORT} is served by {runtime}, not the Floword-owned runtime"));
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

#[tauri::command]
pub fn get_donut_runtime_status(supervisor: tauri::State<'_, RuntimeSupervisor>) -> RuntimeSupervisorStatus {
  supervisor.status()
}

fn resolve_runtime_executable(app: &AppHandle) -> Option<PathBuf> {
  let mut candidates = Vec::new();
  if let Some(path) = std::env::var_os("FLOWORD_DONUT_RUNTIME_EXE") {
    candidates.push(PathBuf::from(path));
  }
  candidates.push(PathBuf::from(r"D:\capcutpolot\donutbrowser\src-tauri\target\debug\floword-donut-runtime.exe"));
  candidates.push(PathBuf::from(r"D:\capcutpolot\artcraft\resources\donut-runtime\floword-donut-runtime.exe"));
  candidates.push(PathBuf::from(r"D:\capcutpolot\artcraft\target\debug\resources\donut-runtime\floword-donut-runtime.exe"));

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

fn port_open() -> bool {
  format!("{HOST}:{PORT}").to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).and_then(|address| TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()).is_some()
}

fn runtime_health() -> Option<String> {
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
  serde_json::from_str::<serde_json::Value>(body).ok()?.get("runtime")?.as_str().map(str::to_string)
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
