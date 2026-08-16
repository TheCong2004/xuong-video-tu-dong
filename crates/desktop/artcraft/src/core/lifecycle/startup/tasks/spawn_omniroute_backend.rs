//! Starts OmniRoute AI Router backend as an auto-started core service on ArtCraft boot.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use log::{error, info, warn};
use tauri::{AppHandle, Manager};

use super::background_command::background_command;

const DEFAULT_PORT: u16 = 20128;
const HEALTH_PATH: &str = "/api/health/ping";
// The production instrumentation hook initializes the local database and
// provider/model schedulers before route handlers become available. On a cold
// Windows start this has measured around two minutes, especially when optional
// external catalog probes must time out. This deadline validates the real health
// endpoint; it does not mark the service ready early.
const READY_DEADLINE: Duration = Duration::from_secs(180);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Routes compiled ahead of the first user click. In dev mode Next compiles a
/// route only when it is first requested, so without this the ~20s webpack
/// compile happens while the user stares at the OmniRoute loading spinner.
/// Requesting them at boot moves that cost into idle startup time instead.
const PREWARM_ROUTES: &[&str] = &["/login", "/", "/dashboard"];
const PREWARM_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Managed Tauri state — killed cleanly when ArtCraft process exits (Drop).
pub struct OmniRouteProcess {
  child: Mutex<Option<Child>>,
}

impl Drop for OmniRouteProcess {
  fn drop(&mut self) {
    self.stop();
  }
}

impl OmniRouteProcess {
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
  info!("Stopping embedded OmniRoute AI Router (pid={pid})");
  #[cfg(target_os = "windows")]
  {
    let _ = background_command(Command::new("taskkill")).args(["/PID", &pid.to_string(), "/T", "/F"]).output();
  }
  let _ = child.kill();
  let _ = child.wait();
}

pub fn port_open(port: u16) -> bool {
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
  status_ok && response.contains("\"status\":\"ok\"")
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

fn resolve_node_executable(dir: &Path, standalone: bool) -> Result<PathBuf, String> {
  let bundled = dir.join(if cfg!(target_os = "windows") { "node.exe" } else { "node" });
  if bundled.is_file() {
    return Ok(bundled);
  }
  if standalone {
    return Err(format!("Bundled Node runtime missing: {}", bundled.display()));
  }

  #[cfg(target_os = "windows")]
  {
    if let Ok(output) = background_command(Command::new("where.exe")).arg("node").output() {
      if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
          let trimmed = line.trim();
          if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
              return Ok(p);
            }
          }
        }
      }
    }
  }
  Ok(PathBuf::from("node"))
}

fn is_omniroute_dir(p: &Path) -> bool {
  p.is_dir() && (p.join("package.json").exists() || p.join("bin").join("omniroute.mjs").exists() || p.join("server.js").exists())
}

fn resolve_omniroute_dir(app: &AppHandle) -> Option<PathBuf> {
  if let Ok(p) = std::env::var("OMNIROUTE_DIR") {
    let pb = PathBuf::from(p);
    if is_omniroute_dir(&pb) {
      return Some(pb);
    }
  }

  let mut candidates: Vec<PathBuf> = Vec::new();

  // Bundled with installed / portable app
  if let Some(dir) = exe_dir() {
    push_if(&mut candidates, dir.join("OmniRoute"));
    push_if(&mut candidates, dir.join("resources").join("OmniRoute"));
    push_if(&mut candidates, dir.join("resources").join("pages").join("OmniRoute"));
    if let Some(parent) = dir.parent() {
      push_if(&mut candidates, parent.join("OmniRoute"));
    }
  }
  if let Some(res) = resource_dir(app) {
    push_if(&mut candidates, res.join("OmniRoute"));
    push_if(&mut candidates, res.join("pages").join("OmniRoute"));
  }

  // Repo / compile-time layout (dev + build machine)
  let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  push_if(&mut candidates, manifest.join("../../../frontend/apps/artcraft/app/src/pages/OmniRoute"));
  push_if(&mut candidates, manifest.join("../../../../frontend/apps/artcraft/app/src/pages/OmniRoute"));

  if let Ok(cwd) = std::env::current_dir() {
    push_if(&mut candidates, cwd.join("frontend/apps/artcraft/app/src/pages/OmniRoute"));
    push_if(&mut candidates, cwd.join("app/src/pages/OmniRoute"));
  }

  for c in candidates {
    if let Ok(c) = c.canonicalize() {
      if is_omniroute_dir(&c) {
        return Some(c);
      }
    } else if is_omniroute_dir(&c) {
      return Some(c);
    }
  }
  None
}

fn spawn_omniroute_from_dir(dir: &Path) -> Result<Child, String> {
  let dev_script = dir.join("scripts").join("dev").join("run-next.mjs");
  let bin_script = dir.join("bin").join("omniroute.mjs");
  let standalone_script = dir.join("server.js");
  let node_exe = resolve_node_executable(dir, standalone_script.is_file())?;

  let mut command = background_command(Command::new(&node_exe));
  command.current_dir(dir);
  command.env("PORT", DEFAULT_PORT.to_string());
  command.env("OMNIROUTE_PORT", DEFAULT_PORT.to_string());
  command.env("OMNIROUTE_USE_TURBOPACK", "0");
  // The embedded desktop runtime needs core HTTP routing immediately. Optional
  // schedulers perform external catalog/health work and can block Next's
  // instrumentation hook for minutes despite being documented as background.
  command.env("OMNIROUTE_DISABLE_BACKGROUND_SERVICES", "true");

  if standalone_script.is_file() {
    command.arg("server.js");
  } else if dev_script.is_file() {
    command.arg("scripts/dev/run-next.mjs").arg("dev");
  } else if bin_script.is_file() {
    command.arg("bin/omniroute.mjs");
  } else {
    command.arg("scripts/dev/run-next.mjs").arg("dev");
  }

  let log_dir = std::env::temp_dir().join("artcraft-omniroute");
  let _ = std::fs::create_dir_all(&log_dir);
  let stdout_path = log_dir.join("omniroute.stdout.log");
  let stderr_path = log_dir.join("omniroute.stderr.log");

  info!("[backend][omniroute] executable={}", node_exe.display());
  info!("[backend][omniroute] args={:?}", command.get_args().collect::<Vec<_>>());
  info!("[backend][omniroute] cwd={}", dir.display());
  info!("[backend][omniroute] stdout={} stderr={}", stdout_path.display(), stderr_path.display());

  if let (Ok(stdout), Ok(stderr)) = (OpenOptions::new().create(true).append(true).open(&stdout_path), OpenOptions::new().create(true).append(true).open(&stderr_path)) {
    command.stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
  } else {
    command.stdout(Stdio::null()).stderr(Stdio::null());
  }

  command.spawn().map_err(|e| format!("Failed to spawn OmniRoute from {}: {e}", dir.display()))
}

fn wait_for_backend_ready(process: &OmniRouteProcess, port: u16) -> bool {
  let deadline = std::time::Instant::now() + READY_DEADLINE;
  let mut attempt = 0_u32;
  loop {
    attempt += 1;
    if health_ready(port) {
      info!("[backend][omniroute] READY http://127.0.0.1:{port}{HEALTH_PATH}");
      return true;
    }
    match process.child.lock() {
      Ok(mut guard) => match guard.as_mut().map(Child::try_wait) {
        Some(Ok(Some(status))) => {
          // try_wait has reaped this PID. Remove it immediately so shutdown can
          // never target a subsequently reused PID.
          guard.take();
          error!("[backend][omniroute] exited before readiness: {status}");
          return false;
        },
        Some(Err(error)) => warn!("[backend][omniroute] process status failed: {error}"),
        Some(Ok(None)) => {},
        None => {
          error!("[backend][omniroute] process ownership disappeared during readiness");
          return false;
        },
      },
      Err(error) => {
        error!("[backend][omniroute] process lock poisoned: {error}");
        return false;
      },
    }
    if std::time::Instant::now() >= deadline {
      warn!("[backend][omniroute] readiness timed out after {}s: http://127.0.0.1:{port}{HEALTH_PATH}", READY_DEADLINE.as_secs());
      if let Ok(mut guard) = process.child.lock() {
        if let Some(mut child) = guard.take() {
          terminate_child_tree(&mut child);
        }
      }
      return false;
    }
    if attempt == 1 || attempt % 10 == 0 {
      info!("[backend][omniroute] readiness attempt {attempt}: http://127.0.0.1:{port}{HEALTH_PATH}");
    }
    std::thread::sleep(READY_POLL_INTERVAL);
  }
}

/// Issues a plain blocking HTTP/1.1 GET and waits for the response headers.
///
/// Deliberately raw TCP rather than reqwest: this runs on a bare `std::thread`
/// with no Tokio runtime, and the crate's reqwest build has no blocking feature.
/// The response body is irrelevant — we only need Next to have finished
/// compiling the route, which it does before writing the status line.
fn prewarm_route(port: u16, route: &str) -> Result<String, String> {
  use std::io::{BufRead, BufReader, Write};

  let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|e| format!("bad address: {e}"))?;

  let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| format!("connect failed: {e}"))?;
  stream.set_read_timeout(Some(PREWARM_REQUEST_TIMEOUT)).map_err(|e| format!("set_read_timeout failed: {e}"))?;
  stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|e| format!("set_write_timeout failed: {e}"))?;

  let request = format!("GET {route} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUser-Agent: ArtCraft-Prewarm\r\nAccept: text/html\r\nConnection: close\r\n\r\n");
  stream.write_all(request.as_bytes()).map_err(|e| format!("write failed: {e}"))?;
  stream.flush().map_err(|e| format!("flush failed: {e}"))?;

  let mut reader = BufReader::new(stream);
  let mut status_line = String::new();
  reader.read_line(&mut status_line).map_err(|e| format!("read failed: {e}"))?;

  if status_line.is_empty() {
    return Err("empty response".to_string());
  }
  Ok(status_line.trim().to_string())
}

/// Compiles the hot OmniRoute pages while the user is still elsewhere in the app.
fn prewarm_routes(port: u16) {
  for route in PREWARM_ROUTES {
    let started = std::time::Instant::now();
    match prewarm_route(port, route) {
      Ok(status) => info!("OmniRoute prewarm {route} -> {status} in {:?}", started.elapsed()),
      // Prewarming is an optimisation, never a boot requirement: a redirect,
      // an auth-gated route or a transient failure must not affect startup.
      Err(e) => warn!("OmniRoute prewarm {route} failed after {:?}: {e}", started.elapsed()),
    }
  }
  info!("OmniRoute prewarm finished");
}

/// Runs the prewarm off the caller's thread so the remaining boot tasks
/// (capcut-mate, auxiliary backends) are not delayed by route compilation.
fn spawn_prewarm_thread(port: u16) {
  std::thread::spawn(move || prewarm_routes(port));
}

fn manage_empty(app: &AppHandle) {
  app.manage(OmniRouteProcess { child: Mutex::new(None) });
}

/// Call from Tauri setup. Spawns OmniRoute in background on boot without UI interaction.
pub fn spawn_omniroute_backend(app: &AppHandle) {
  if port_open(DEFAULT_PORT) {
    if !health_ready(DEFAULT_PORT) {
      error!("[backend][omniroute] PORT_IN_USE: :{DEFAULT_PORT} is occupied but {HEALTH_PATH} is not healthy");
      manage_empty(app);
      return;
    }
    info!("OmniRoute already listening on :{DEFAULT_PORT} — reuse (no spawn)");
    manage_empty(app);
    // A reused server may still be a cold dev server with nothing compiled.
    spawn_prewarm_thread(DEFAULT_PORT);
    return;
  }

  let Some(dir) = resolve_omniroute_dir(app) else {
    warn!("OmniRoute directory not found; skipping auto-start");
    manage_empty(app);
    return;
  };

  info!("[production-paths][omniroute] exe_dir={:?} resource_dir={:?} cwd={:?}", exe_dir(), resource_dir(app), std::env::current_dir().ok());
  info!("[backend][omniroute] runtime={}", dir.display());

  match spawn_omniroute_from_dir(&dir) {
    Ok(child) => {
      info!("Started embedded OmniRoute AI Router from {} (pid={})", dir.display(), child.id());
      app.manage(OmniRouteProcess { child: Mutex::new(Some(child)) });
      let ready = wait_for_backend_ready(app.state::<OmniRouteProcess>().inner(), DEFAULT_PORT);
      if ready {
        spawn_prewarm_thread(DEFAULT_PORT);
      }
    },
    Err(e) => {
      error!("Failed to start OmniRoute AI Router: {e}");
      manage_empty(app);
    },
  }
}
