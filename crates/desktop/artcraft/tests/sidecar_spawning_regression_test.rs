use std::fs;
use std::path::Path;
use std::process::Command;
use artcraft_app_lib::core::lifecycle::startup::tasks::background_command::background_command;

#[test]
fn test_background_command_preserves_command_integrity() {
  let cmd = background_command(Command::new("test_binary"));
  let program = cmd.get_program();
  assert_eq!(program, "test_binary");
}

#[test]
fn test_sidecar_startup_files_use_background_command_for_all_spawns() {
  let manifest_dir = env!("CARGO_MANIFEST_DIR");
  let tasks_dir = Path::new(manifest_dir).join("src/core/lifecycle/startup/tasks");

  let sidecar_files = ["spawn_capcut_mate_backend.rs", "spawn_omniroute_backend.rs", "spawn_auxiliary_backends.rs"];

  for file_name in &sidecar_files {
    let file_path = tasks_dir.join(file_name);
    assert!(file_path.exists(), "Target sidecar file does not exist: {}", file_path.display());

    let content = fs::read_to_string(&file_path).unwrap_or_else(|e| panic!("Failed to read {}: {e}", file_path.display()));

    assert!(content.contains("background_command"), "File {} does not import or use background_command", file_name);

    for (line_idx, line) in content.lines().enumerate() {
      let trimmed = line.trim();
      // If line instantiates Command::new, it must be wrapped in background_command
      if trimmed.contains("Command::new(") {
        assert!(trimmed.contains("background_command(Command::new("), "Unshielded Command::new found at {}:{} -> {}", file_name, line_idx + 1, trimmed);
      }
    }
  }
}
