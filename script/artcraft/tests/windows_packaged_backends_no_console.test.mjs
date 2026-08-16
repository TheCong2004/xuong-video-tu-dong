import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const tasksDir = path.resolve(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "crates",
  "desktop",
  "artcraft",
  "src",
  "core",
  "lifecycle",
  "startup",
  "tasks"
);

const readTask = (name) => fs.readFileSync(path.join(tasksDir, name), "utf8");

test("the shared Windows background-command helper suppresses console windows", () => {
  const source = readTask("background_command.rs");

  assert.match(source, /std::os::windows::process::CommandExt/);
  assert.match(source, /CREATE_NO_WINDOW\s*:\s*u32\s*=\s*0x08000000/);
  assert.match(source, /command\.creation_flags\(CREATE_NO_WINDOW\)/);
});

test("all packaged backend launch and cleanup commands use the shared helper", () => {
  for (const file of [
    "spawn_capcut_mate_backend.rs",
    "spawn_omniroute_backend.rs",
    "spawn_auxiliary_backends.rs",
  ]) {
    const source = readTask(file);
    const directCommands = [...source.matchAll(/Command::new\(/g)];
    const hiddenCommands = [...source.matchAll(/background_command\(Command::new\(/g)];

    assert.ok(directCommands.length > 0, `${file} should exercise process launching`);
    assert.equal(
      hiddenCommands.length,
      directCommands.length,
      `${file} must wrap every Command::new call with background_command`
    );
  }
});
