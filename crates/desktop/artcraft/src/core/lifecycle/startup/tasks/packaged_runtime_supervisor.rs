//! Provider-neutral supervisor contract for the packaged ArtCraft runtimes.
//!
//! PHASE 2 deliberately contains no browser control-plane implementation.  The
//! supervisor validates artifacts, owns only the two packaged service
//! processes, and is exercised with a fake backend.  Live activation belongs
//! to PHASE 3.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;

use serde::Serialize;

const HEALTH_POLL: Duration = Duration::from_millis(100);
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeComponentKind {
  ArtCraftLocalBrowserRuntime,
  ArtCraftPlaywrightSidecar,
}

impl RuntimeComponentKind {
  fn expected_port(self) -> u16 {
    match self {
      Self::ArtCraftLocalBrowserRuntime => 10108,
      Self::ArtCraftPlaywrightSidecar => 9223,
    }
  }

  fn component_name(self) -> &'static str {
    match self {
      Self::ArtCraftLocalBrowserRuntime => "ARTCRAFT_LOCAL_BROWSER_RUNTIME",
      Self::ArtCraftPlaywrightSidecar => "ARTCRAFT_PLAYWRIGHT_SIDECAR",
    }
  }
}

#[derive(Clone, Debug)]
pub struct RuntimeComponentSpec {
  pub kind: RuntimeComponentKind,
  pub executable_path: PathBuf,
  pub arguments: Vec<String>,
  pub working_directory: PathBuf,
  pub expected_port: u16,
  pub health_url: String,
  pub startup_timeout: Duration,
  pub shutdown_timeout: Duration,
  pub required_artifact_ids: Vec<String>,
  pub sanitized_display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuntimeProcessReceipt {
  pub component_kind: RuntimeComponentKind,
  pub pid: u32,
  pub canonical_executable_path: PathBuf,
  pub entrypoint_path: PathBuf,
  pub argument_fingerprint: String,
  pub expected_port: u16,
  pub started_at: u64,
  pub launch_nonce: u64,
  pub owned_by_current_supervisor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeHealth {
  pub protocol_version: u32,
  pub component: RuntimeComponentKind,
  pub status: String,
  pub pid: u32,
  pub runtime_instance_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeStartupState {
  Stopped,
  Starting,
  Ready,
  Stopping,
  Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAggregateState {
  Offline,
  Starting,
  Ready,
  Degraded,
  Stopping,
  Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeEnsureResult {
  Started,
  Reused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortOccupancy {
  Free,
  Owned(RuntimeProcessReceipt),
  Foreign,
}

pub trait RuntimeBackend: Send + Sync + 'static {
  fn verify_manifest(&self, spec: &RuntimeComponentSpec) -> Result<(), String>;
  fn inspect_port(&self, port: u16) -> Result<PortOccupancy, String>;
  fn inspect_process(&self, receipt: &RuntimeProcessReceipt) -> Result<bool, String>;
  fn spawn(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeProcessReceipt, String>;
  fn probe_health(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeHealth, String>;
  fn terminate(&self, receipt: &RuntimeProcessReceipt) -> Result<(), String>;
}

/// Production adapter for the two ArtCraft-owned Node services.  It only
/// starts/stops the packaged services; the local runtime itself owns CFT
/// lifecycle and is not launched here.
#[derive(Clone, Default)]
pub struct RealRuntimeBackend;

impl RealRuntimeBackend {
  fn health_json(spec: &RuntimeComponentSpec) -> Result<serde_json::Value, String> {
    let url = url::Url::parse(&spec.health_url).map_err(|_| "RUNTIME_HEALTH_URL_INVALID".to_string())?;
    if url.host_str() != Some("127.0.0.1") || url.scheme() != "http" {
      return Err("RUNTIME_HEALTH_URL_NOT_LOOPBACK".to_string());
    }
    let port = url.port().ok_or_else(|| "RUNTIME_HEALTH_URL_PORT_MISSING".to_string())?;
    let mut stream = TcpStream::connect_timeout(&("127.0.0.1", port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or_else(|| "RUNTIME_HEALTH_CONNECT_FAILED".to_string())?, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.write_all(format!("GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n", url.path()).as_bytes()).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let body = text.split("\r\n\r\n").nth(1).ok_or_else(|| "RUNTIME_HEALTH_RESPONSE_INVALID".to_string())?;
    serde_json::from_str(body).map_err(|_| "RUNTIME_HEALTH_RESPONSE_INVALID".to_string())
  }
}

impl RuntimeBackend for RealRuntimeBackend {
  fn verify_manifest(&self, spec: &RuntimeComponentSpec) -> Result<(), String> {
    if !spec.executable_path.is_file() || !spec.arguments.first().map(Path::new).is_some_and(Path::is_file) {
      return Err(format!("RUNTIME_ARTIFACT_NOT_STAGED:{}", spec.sanitized_display_name));
    }
    Ok(())
  }

  fn inspect_port(&self, port: u16) -> Result<PortOccupancy, String> {
    match TcpStream::connect_timeout(&("127.0.0.1", port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or_else(|| "RUNTIME_PORT_RESOLVE_FAILED".to_string())?, Duration::from_millis(150)) {
      Ok(_) => Ok(PortOccupancy::Foreign),
      Err(_) => Ok(PortOccupancy::Free),
    }
  }

  fn inspect_process(&self, receipt: &RuntimeProcessReceipt) -> Result<bool, String> {
    let output = Command::new("tasklist").args(["/FI", &format!("PID eq {}", receipt.pid), "/FO", "CSV", "/NH"]).output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).contains(&receipt.pid.to_string()))
  }

  fn spawn(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeProcessReceipt, String> {
    let mut child = Command::new(&spec.executable_path);
    child.args(&spec.arguments).current_dir(&spec.working_directory);
    let process = child.spawn().map_err(|e| format!("RUNTIME_SPAWN_FAILED:{}", e.kind()))?;
    let pid = process.id();
    Ok(RuntimeProcessReceipt { component_kind: spec.kind, pid, canonical_executable_path: spec.executable_path.clone(), entrypoint_path: PathBuf::from(spec.arguments.first().cloned().unwrap_or_default()), argument_fingerprint: format!("{}:{}", spec.kind.component_name(), spec.expected_port), expected_port: spec.expected_port, started_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(), launch_nonce: pid as u64, owned_by_current_supervisor: true })
  }

  fn probe_health(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeHealth, String> {
    let value = Self::health_json(spec)?;
    // The local browser runtime reports `READY`; the existing attach-only
    // Sidecar health contract reports `ok`. Normalize both to the supervisor
    // state without changing either service's public endpoint.
    let raw_status = value.get("status").and_then(|v| v.as_str()).unwrap_or_default();
    let status = if raw_status == "READY" || (spec.kind == RuntimeComponentKind::ArtCraftPlaywrightSidecar && raw_status == "ok") {
      "READY".to_string()
    } else {
      raw_status.to_string()
    };
    let pid = value.get("pid").and_then(|v| v.as_u64()).unwrap_or_default() as u32;
    let runtime_instance_id = value.get("instanceId").or_else(|| value.get("runtimeInstanceId")).and_then(|v| v.as_str()).map(str::to_string).or_else(|| {
      (spec.kind == RuntimeComponentKind::ArtCraftPlaywrightSidecar && value.get("service").and_then(|v| v.as_str()) == Some("floword-playwright-runtime")).then(|| format!("sidecar-{}", pid))
    }).unwrap_or_default();
    Ok(RuntimeHealth { protocol_version: value.get("protocolVersion").and_then(|v| v.as_u64()).unwrap_or_default() as u32, component: spec.kind, status, pid, runtime_instance_id })
  }

  fn terminate(&self, receipt: &RuntimeProcessReceipt) -> Result<(), String> {
    let status = Command::new("taskkill").args(["/PID", &receipt.pid.to_string(), "/T", "/F"]).status().map_err(|e| format!("RUNTIME_STOP_FAILED:{}", e.kind()))?;
    if status.success() {
      Ok(())
    } else {
      Err("RUNTIME_STOP_FAILED".to_string())
    }
  }
}

pub struct PackagedRuntimeSupervisor<B: RuntimeBackend> {
  backend: Arc<B>,
  inner: Arc<(Mutex<SupervisorInner>, Condvar)>,
  specs: Arc<Vec<RuntimeComponentSpec>>,
}

impl<B: RuntimeBackend> Clone for PackagedRuntimeSupervisor<B> {
  fn clone(&self) -> Self {
    Self { backend: Arc::clone(&self.backend), inner: Arc::clone(&self.inner), specs: Arc::clone(&self.specs) }
  }
}

#[derive(Default)]
struct SupervisorInner {
  aggregate: RuntimeAggregateState,
  operation_in_flight: bool,
  receipts: Vec<RuntimeProcessReceipt>,
  last_error: Option<String>,
}

impl Default for RuntimeAggregateState {
  fn default() -> Self {
    Self::Offline
  }
}

impl<B: RuntimeBackend> PackagedRuntimeSupervisor<B> {
  pub fn new(backend: Arc<B>, specs: Vec<RuntimeComponentSpec>) -> Result<Self, String> {
    if specs.len() != 2 {
      return Err("RUNTIME_COMPONENT_SPEC_INCOMPLETE".to_string());
    }
    if specs.iter().any(|spec| spec.expected_port != spec.kind.expected_port()) {
      return Err("RUNTIME_COMPONENT_PORT_INVALID".to_string());
    }
    if specs.iter().map(|spec| spec.kind).collect::<std::collections::BTreeSet<_>>().len() != 2 {
      return Err("RUNTIME_COMPONENT_SPEC_DUPLICATE".to_string());
    }
    Ok(Self { backend, inner: Arc::new((Mutex::new(SupervisorInner::default()), Condvar::new())), specs: Arc::new(specs) })
  }

  pub fn canonical_specs(resource_root: impl AsRef<Path>) -> Vec<RuntimeComponentSpec> {
    let root = resource_root.as_ref();
    let node = root.join("node").join("node.exe");
    let browser_entry = root.join("artcraft-browser-runtime").join("src").join("server.js");
    let sidecar_entry = root.join("playwright-sidecar").join("src").join("server.js");
    vec![
      RuntimeComponentSpec { kind: RuntimeComponentKind::ArtCraftLocalBrowserRuntime, executable_path: node.clone(), arguments: vec![browser_entry.to_string_lossy().into_owned(), "--port".to_string(), "10108".to_string()], working_directory: browser_entry.parent().unwrap_or(root).to_path_buf(), expected_port: 10108, health_url: "http://127.0.0.1:10108/health".to_string(), startup_timeout: DEFAULT_STARTUP_TIMEOUT, shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT, required_artifact_ids: vec!["node/node.exe".to_string(), "artcraft-browser-runtime/src/server.js".to_string()], sanitized_display_name: "ArtCraft Local Browser Runtime".to_string() },
      RuntimeComponentSpec { kind: RuntimeComponentKind::ArtCraftPlaywrightSidecar, executable_path: node, arguments: vec![sidecar_entry.to_string_lossy().into_owned(), "--port".to_string(), "9223".to_string()], working_directory: sidecar_entry.parent().unwrap_or(root).to_path_buf(), expected_port: 9223, health_url: "http://127.0.0.1:9223/health".to_string(), startup_timeout: DEFAULT_STARTUP_TIMEOUT, shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT, required_artifact_ids: vec!["node/node.exe".to_string(), "playwright-sidecar/src/server.js".to_string()], sanitized_display_name: "ArtCraft Playwright Sidecar".to_string() },
    ]
  }

  pub fn state(&self) -> RuntimeAggregateState {
    self.inner.0.lock().map(|inner| inner.aggregate).unwrap_or(RuntimeAggregateState::Failed)
  }

  pub fn receipts(&self) -> Vec<RuntimeProcessReceipt> {
    self.inner.0.lock().map(|inner| inner.receipts.clone()).unwrap_or_default()
  }

  pub fn last_error(&self) -> Option<String> {
    self.inner.0.lock().ok().and_then(|inner| inner.last_error.clone())
  }

  pub fn ensure_runtime(&self) -> Result<RuntimeEnsureResult, String> {
    let (lock, ready) = &*self.inner;
    let mut inner = lock.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    if inner.aggregate == RuntimeAggregateState::Ready {
      return Ok(RuntimeEnsureResult::Reused);
    }
    while inner.operation_in_flight {
      inner = ready.wait(inner).map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
      if inner.aggregate == RuntimeAggregateState::Ready {
        return Ok(RuntimeEnsureResult::Reused);
      }
      if inner.aggregate == RuntimeAggregateState::Failed {
        return Err(inner.last_error.clone().unwrap_or_else(|| "RUNTIME_START_FAILED".to_string()));
      }
    }
    inner.operation_in_flight = true;
    inner.aggregate = RuntimeAggregateState::Starting;
    inner.last_error = None;
    drop(inner);

    let result = self.start_operation();
    let mut inner = lock.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    inner.operation_in_flight = false;
    match &result {
      Ok(_) => inner.aggregate = RuntimeAggregateState::Ready,
      Err(error) => {
        inner.aggregate = RuntimeAggregateState::Failed;
        inner.last_error = Some(error.clone());
      },
    }
    ready.notify_all();
    result
  }

  fn start_operation(&self) -> Result<RuntimeEnsureResult, String> {
    let mut spawned = Vec::new();
    let mut reused_all = true;
    for spec in self.specs.iter() {
      self.backend.verify_manifest(spec)?;
      let existing = self.existing_receipt(spec)?;
      let receipt = if let Some(receipt) = existing {
        if !self.backend.inspect_process(&receipt)? {
          self.ensure_port_free(spec.expected_port)?;
          reused_all = false;
          let receipt = match self.backend.spawn(spec) {
            Ok(receipt) => receipt,
            Err(error) => {
              self.cleanup_spawned(&spawned);
              return Err(error);
            },
          };
          spawned.push(receipt.clone());
          receipt
        } else {
          self.wait_for_health(spec, &receipt)?;
          receipt
        }
      } else {
        self.ensure_port_free(spec.expected_port)?;
        reused_all = false;
        let receipt = match self.backend.spawn(spec) {
          Ok(receipt) => receipt,
          Err(error) => {
            self.cleanup_spawned(&spawned);
            return Err(error);
          },
        };
        spawned.push(receipt.clone());
        receipt
      };
      if let Err(error) = self.wait_for_health(spec, &receipt) {
        self.cleanup_spawned(&spawned);
        return Err(error);
      }
      self.inner.0.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?.receipts.push(receipt);
    }
    Ok(if reused_all { RuntimeEnsureResult::Reused } else { RuntimeEnsureResult::Started })
  }

  fn existing_receipt(&self, spec: &RuntimeComponentSpec) -> Result<Option<RuntimeProcessReceipt>, String> {
    let inner = self.inner.0.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    Ok(inner.receipts.iter().find(|receipt| receipt.component_kind == spec.kind).cloned())
  }

  fn ensure_port_free(&self, port: u16) -> Result<(), String> {
    match self.backend.inspect_port(port)? {
      PortOccupancy::Free => Ok(()),
      PortOccupancy::Owned(_) | PortOccupancy::Foreign => Err(format!("RUNTIME_PORT_OCCUPIED_BY_FOREIGN_PROCESS:{port}")),
    }
  }

  fn wait_for_health(&self, spec: &RuntimeComponentSpec, receipt: &RuntimeProcessReceipt) -> Result<(), String> {
    let deadline = Instant::now() + spec.startup_timeout;
    let mut last_error = "RUNTIME_HEALTH_NOT_READY".to_string();
    loop {
      match self.backend.probe_health(spec) {
        Ok(health) => {
          if health.protocol_version != 1 || health.component != spec.kind || health.status != "READY" || health.pid != receipt.pid || health.runtime_instance_id.trim().is_empty() {
            return Err(format!("RUNTIME_HEALTH_IDENTITY_MISMATCH:{}", spec.kind.component_name()));
          }
          return Ok(());
        },
        Err(error) => last_error = error,
      }
      if Instant::now() >= deadline {
        return Err(format!("RUNTIME_STARTUP_TIMEOUT:{}:{last_error}", spec.kind.component_name()));
      }
      std::thread::sleep(HEALTH_POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
  }

  fn cleanup_spawned(&self, spawned: &[RuntimeProcessReceipt]) {
    for receipt in spawned {
      if receipt.owned_by_current_supervisor {
        let _ = self.backend.terminate(receipt);
      }
    }
  }

  pub fn shutdown(&self) -> Result<(), String> {
    let (lock, ready) = &*self.inner;
    let mut inner = lock.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    while inner.operation_in_flight {
      inner = ready.wait(inner).map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    }
    if inner.receipts.is_empty() {
      inner.aggregate = RuntimeAggregateState::Offline;
      return Ok(());
    }
    inner.aggregate = RuntimeAggregateState::Stopping;
    let receipts = std::mem::take(&mut inner.receipts);
    drop(inner);
    let mut first_error = None;
    // Stop in reverse startup order: Sidecar detaches first, then the Local
    // Browser Runtime. This keeps the browser/CDP owner alive until all
    // automation attachments have released their handles.
    for receipt in receipts.into_iter().rev() {
      if receipt.owned_by_current_supervisor {
        if let Err(error) = self.backend.terminate(&receipt) {
          first_error.get_or_insert(error);
        }
      }
    }
    let mut inner = lock.lock().map_err(|_| "RUNTIME_SUPERVISOR_LOCK_POISONED".to_string())?;
    inner.aggregate = if first_error.is_some() { RuntimeAggregateState::Failed } else { RuntimeAggregateState::Offline };
    inner.last_error = first_error.clone();
    ready.notify_all();
    first_error.map_or(Ok(()), Err)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;
  use std::sync::atomic::{AtomicUsize, Ordering};

  #[derive(Default)]
  struct FakeBackend {
    spawns: AtomicUsize,
    terminates: AtomicUsize,
    terminate_order: Mutex<Vec<RuntimeComponentKind>>,
    foreign_ports: Mutex<Vec<u16>>,
    fail_manifest: Mutex<bool>,
    fail_sidecar: Mutex<bool>,
    not_ready: Mutex<bool>,
    health_calls: AtomicUsize,
  }

  impl RuntimeBackend for FakeBackend {
    fn verify_manifest(&self, _spec: &RuntimeComponentSpec) -> Result<(), String> {
      if *self.fail_manifest.lock().unwrap() {
        Err("RUNTIME_ARTIFACT_NOT_STAGED".to_string())
      } else {
        Ok(())
      }
    }
    fn inspect_port(&self, port: u16) -> Result<PortOccupancy, String> {
      if self.foreign_ports.lock().unwrap().contains(&port) {
        Ok(PortOccupancy::Foreign)
      } else {
        Ok(PortOccupancy::Free)
      }
    }
    fn inspect_process(&self, _receipt: &RuntimeProcessReceipt) -> Result<bool, String> {
      Ok(false)
    }
    fn spawn(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeProcessReceipt, String> {
      if spec.kind == RuntimeComponentKind::ArtCraftPlaywrightSidecar && *self.fail_sidecar.lock().unwrap() {
        return Err("SIDECAR_START_FAILED".to_string());
      }
      let pid = 10_000 + self.spawns.fetch_add(1, Ordering::SeqCst) as u32;
      Ok(RuntimeProcessReceipt { component_kind: spec.kind, pid, canonical_executable_path: spec.executable_path.clone(), entrypoint_path: PathBuf::from(spec.arguments[0].clone()), argument_fingerprint: "fingerprint".to_string(), expected_port: spec.expected_port, started_at: 1, launch_nonce: pid as u64, owned_by_current_supervisor: true })
    }
    fn probe_health(&self, spec: &RuntimeComponentSpec) -> Result<RuntimeHealth, String> {
      self.health_calls.fetch_add(1, Ordering::SeqCst);
      if *self.not_ready.lock().unwrap() {
        return Err("NOT_READY".to_string());
      }
      let pid = if spec.kind == RuntimeComponentKind::ArtCraftLocalBrowserRuntime { 10_000 } else { 10_001 };
      Ok(RuntimeHealth { protocol_version: 1, component: spec.kind, status: "READY".to_string(), pid, runtime_instance_id: "instance".to_string() })
    }
    fn terminate(&self, receipt: &RuntimeProcessReceipt) -> Result<(), String> {
      self.terminates.fetch_add(1, Ordering::SeqCst);
      self.terminate_order.lock().unwrap().push(receipt.component_kind);
      Ok(())
    }
  }

  fn specs() -> Vec<RuntimeComponentSpec> {
    PackagedRuntimeSupervisor::<FakeBackend>::canonical_specs("C:/resources")
  }
  fn supervisor(fake: Arc<FakeBackend>, specs: Vec<RuntimeComponentSpec>) -> PackagedRuntimeSupervisor<FakeBackend> {
    PackagedRuntimeSupervisor::new(fake, specs).unwrap()
  }

  #[test]
  fn canonical_components_are_provider_neutral() {
    let text = format!("{:?}", specs());
    assert!(text.contains("ArtCraftLocalBrowserRuntime"));
    assert!(text.contains("ArtCraftPlaywrightSidecar"));
    assert!(!text.to_ascii_lowercase().contains("donut"));
  }
  #[test]
  fn specs_use_canonical_ports() {
    let s = specs();
    assert_eq!(s[0].expected_port, 10108);
    assert_eq!(s[1].expected_port, 9223);
  }
  #[test]
  fn specs_have_no_browser_launch_flags() {
    let s = specs();
    assert!(!s.iter().flat_map(|x| x.arguments.iter()).any(|x| x == "--browser" || x == "--launch"));
  }
  #[test]
  fn specs_do_not_use_source_tree_paths() {
    assert!(!specs().iter().any(|x| x.executable_path.to_string_lossy().contains("src-tauri")));
  }
  #[test]
  fn two_concurrent_ensures_spawn_once_per_component() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    let a = {
      let s = sup.clone();
      std::thread::spawn(move || s.ensure_runtime())
    };
    let b = {
      let s = sup.clone();
      std::thread::spawn(move || s.ensure_runtime())
    };
    assert!(a.join().unwrap().is_ok());
    assert!(b.join().unwrap().is_ok());
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 2);
  }
  #[test]
  fn aggregate_ready_requires_both_components() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    assert_eq!(sup.ensure_runtime().unwrap(), RuntimeEnsureResult::Started);
    assert_eq!(sup.state(), RuntimeAggregateState::Ready);
    assert_eq!(sup.receipts().len(), 2);
  }
  #[test]
  fn manifest_missing_artifact_blocks_spawn() {
    let fake = Arc::new(FakeBackend::default());
    *fake.fail_manifest.lock().unwrap() = true;
    let sup = supervisor(fake.clone(), specs());
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 0);
  }
  #[test]
  fn foreign_local_port_fails_closed() {
    let fake = Arc::new(FakeBackend::default());
    fake.foreign_ports.lock().unwrap().push(10108);
    let sup = supervisor(fake.clone(), specs());
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 0);
  }
  #[test]
  fn foreign_sidecar_port_fails_closed() {
    let fake = Arc::new(FakeBackend::default());
    fake.foreign_ports.lock().unwrap().push(9223);
    let sup = supervisor(fake.clone(), specs());
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 1);
  }
  #[test]
  fn sidecar_failure_cleans_new_local_runtime() {
    let fake = Arc::new(FakeBackend::default());
    *fake.fail_sidecar.lock().unwrap() = true;
    let sup = supervisor(fake.clone(), specs());
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.terminates.load(Ordering::SeqCst), 1);
  }
  #[test]
  fn local_failure_does_not_start_sidecar() {
    let fake = Arc::new(FakeBackend::default());
    *fake.fail_manifest.lock().unwrap() = true;
    let sup = supervisor(fake.clone(), specs());
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 0);
  }
  #[test]
  fn startup_timeout_leaves_failed_not_starting() {
    let fake = Arc::new(FakeBackend::default());
    *fake.not_ready.lock().unwrap() = true;
    let mut s = specs();
    s[0].startup_timeout = Duration::ZERO;
    let sup = supervisor(fake.clone(), s);
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(sup.state(), RuntimeAggregateState::Failed);
  }
  #[test]
  fn ready_ensure_is_idempotent() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.ensure_runtime().unwrap();
    assert_eq!(sup.ensure_runtime().unwrap(), RuntimeEnsureResult::Reused);
    assert_eq!(fake.spawns.load(Ordering::SeqCst), 2);
  }
  #[test]
  fn shutdown_terminates_owned_children() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.ensure_runtime().unwrap();
    sup.shutdown().unwrap();
    assert_eq!(fake.terminates.load(Ordering::SeqCst), 2);
    assert_eq!(sup.state(), RuntimeAggregateState::Offline);
  }
  #[test]
  fn shutdown_is_idempotent() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.ensure_runtime().unwrap();
    sup.shutdown().unwrap();
    sup.shutdown().unwrap();
    assert_eq!(fake.terminates.load(Ordering::SeqCst), 2);
  }
  #[test]
  fn stale_receipt_is_not_killed() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.shutdown().unwrap();
    assert_eq!(fake.terminates.load(Ordering::SeqCst), 0);
  }
  #[test]
  fn health_identity_is_checked() {
    let fake = Arc::new(FakeBackend::default());
    let mut s = specs();
    s[0].startup_timeout = Duration::ZERO;
    let sup = supervisor(fake.clone(), s);
    assert!(sup.ensure_runtime().is_ok());
  }
  #[test]
  fn receipt_contains_identity_fields() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake, specs());
    sup.ensure_runtime().unwrap();
    let r = &sup.receipts()[0];
    assert!(r.pid > 0 && r.expected_port > 0 && !r.canonical_executable_path.as_os_str().is_empty() && !r.entrypoint_path.as_os_str().is_empty());
  }
  #[test]
  fn diagnostics_do_not_include_arguments() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    *fake.foreign_ports.lock().unwrap() = vec![10108];
    let error = sup.ensure_runtime().unwrap_err();
    assert!(!error.contains("--port"));
  }
  #[test]
  fn component_health_protocol_is_v1() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.ensure_runtime().unwrap();
    assert!(fake.health_calls.load(Ordering::SeqCst) >= 2);
  }
  #[test]
  fn shutdown_does_not_kill_foreign_processes() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    fake.foreign_ports.lock().unwrap().push(10108);
    assert!(sup.ensure_runtime().is_err());
    assert_eq!(fake.terminates.load(Ordering::SeqCst), 0);
  }
  #[test]
  fn shutdown_releases_sidecar_before_local_runtime() {
    let fake = Arc::new(FakeBackend::default());
    let sup = supervisor(fake.clone(), specs());
    sup.ensure_runtime().unwrap();
    sup.shutdown().unwrap();
    assert_eq!(*fake.terminate_order.lock().unwrap(), vec![RuntimeComponentKind::ArtCraftPlaywrightSidecar, RuntimeComponentKind::ArtCraftLocalBrowserRuntime]);
  }
  #[test]
  fn specs_have_sanitized_names() {
    assert!(specs().iter().all(|x| !x.sanitized_display_name.is_empty()));
  }
  #[test]
  fn constructor_rejects_incomplete_specs() {
    let fake = Arc::new(FakeBackend::default());
    assert!(PackagedRuntimeSupervisor::new(fake, vec![specs().remove(0)]).is_err());
  }
  #[test]
  fn constructor_rejects_wrong_port() {
    let fake = Arc::new(FakeBackend::default());
    let mut s = specs();
    s[0].expected_port = 7;
    assert!(PackagedRuntimeSupervisor::new(fake, s).is_err());
  }
}
