import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const resolverPath = path.resolve(
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
  "tasks",
  "spawn_capcut_mate_backend.rs"
);

test("packaged CapCut Mate resolver checks the Tauri resources subdirectory", () => {
  const source = fs.readFileSync(resolverPath, "utf8");

  assert.match(
    source,
    /dir\.join\("resources"\)\.join\(SIDECAR_NAME\)/,
    "the installed app keeps capcut-mate-server.exe below <exe-dir>/resources"
  );
  assert.match(
    source,
    /res\.join\("resources"\)\.join\(SIDECAR_NAME\)/,
    "some Tauri versions report the install root as resource_dir"
  );
});
