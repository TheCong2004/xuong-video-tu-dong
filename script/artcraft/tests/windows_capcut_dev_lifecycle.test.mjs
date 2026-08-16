import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const scriptPath = path.resolve(testDir, "..", "windows_capcut_dev.ps1");
const source = fs.readFileSync(scriptPath, "utf8");

test("CapCut dev owns and cleans up the OmniRoute process tree", () => {
  assert.match(source, /function\s+Stop-OmniRouteDevProcess/);
  assert.match(source, /try\s*\{[\s\S]*windows_rust_dev\.ps1[\s\S]*\}\s*finally\s*\{/);
  assert.match(source, /Stop-OmniRouteDevProcess\s+-Process\s+\$omniProcess/);
});
