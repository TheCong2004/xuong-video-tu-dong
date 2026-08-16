import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const scriptPath = path.resolve(
  import.meta.dirname,
  "..",
  "windows_build.ps1"
);

test("packaged OmniRoute smoke uses the same startup environment as desktop", () => {
  const source = fs.readFileSync(scriptPath, "utf8");

  assert.match(
    source,
    /EnvironmentVariables\["OMNIROUTE_PORT"\]\s*=\s*\[string\]\$OmniRouteSmokePort/
  );
  assert.match(
    source,
    /EnvironmentVariables\["OMNIROUTE_DISABLE_BACKGROUND_SERVICES"\]\s*=\s*"true"/
  );
});
