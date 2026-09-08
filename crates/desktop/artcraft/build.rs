fn main() {
  // Keep the ignored Tauri resource copy in lockstep with the tracked worker.
  // Without this staging step `cargo tauri dev` could package/run an older
  // worker even though the source checkout had the PyBind boundary fix.
  println!("cargo:rerun-if-changed=../../../tools/capcut-automation/translation_worker.py");
  println!("cargo:rerun-if-changed=../../../tools/capcut-automation/ocr_rapid_worker.py");
  let resource_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/capcut-automation/scripts");
  for (name, label) in [("translation_worker.py", "TRANSLATION"), ("ocr_rapid_worker.py", "OCR")] {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/capcut-automation").join(name);
    let destination = resource_root.join(name);
    if source.is_file() {
      std::fs::create_dir_all(&resource_root).expect("CAPCUT_WORKER_STAGING_FAILED: directory");
      // A running dev binary can hold the ignored resource copy open on
      // Windows. Do not make check/test commands fail solely because of an
      // old process; the resolver verifies the digest and falls back to the
      // tracked source in development.
      if let Err(error) = std::fs::copy(&source, &destination) {
        println!("cargo:warning=CAPCUT_{label}_WORKER_STAGING_SKIPPED: {error}");
      }
    }
  }
  // Unit tests exercise the Rust pipeline directly and do not need Tauri's
  // resource-copy/package step. Allow them to run while a user-owned dev
  // Sidecar still has a resource file open; normal `cargo tauri dev/build`
  // keeps the full Tauri build path unless this opt-in test flag is set.
  if std::env::var_os("ARTCRAFT_SKIP_TAURI_BUILD").is_none() {
    tauri_build::build();
  }
}
