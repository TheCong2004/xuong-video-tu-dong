import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

test("OmniRoute iframe provides a visible dashboard reset control", () => {
  const source = fs.readFileSync(new URL("./index.tsx", import.meta.url), "utf8");
  assert.match(source, /data-testid="omniroute-dashboard-back-button"/);
  assert.match(source, /resetToDashboard/);
  assert.match(source, /Về OmniRoute/);
});
