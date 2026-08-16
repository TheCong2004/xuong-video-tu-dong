process.env.OMNIROUTE_ENABLE_LIVE_WS = "0";

import test from "node:test";
import assert from "node:assert/strict";
import { isAnonymousLiveWsAllowed } from "@/server/ws/liveServer";

test("embedded auth-disabled dashboard may use LiveWS over loopback", () => {
  assert.equal(isAnonymousLiveWsAllowed(false, "127.0.0.1"), true);
  assert.equal(isAnonymousLiveWsAllowed(false, "::1"), true);
  assert.equal(isAnonymousLiveWsAllowed(false, "::ffff:127.0.0.1"), true);
});

test("anonymous LiveWS remains blocked when auth is required or peer is remote", () => {
  assert.equal(isAnonymousLiveWsAllowed(true, "127.0.0.1"), false);
  assert.equal(isAnonymousLiveWsAllowed(false, "192.168.1.50"), false);
  assert.equal(isAnonymousLiveWsAllowed(false, undefined), false);
});
