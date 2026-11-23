import { test } from "node:test";
import assert from "node:assert/strict";
import { validRuntimeHealth, formatRuntimeHealth, getRuntimeHealth } from "../src/player/runtimeHealth.ts";

const report = {
  schemaVersion: 1,
  activeSessionId: 7,
  sessionsOpened: 9,
  filesLoaded: 8,
  playbackRestarts: 8,
  playbackEnded: 4,
  playbackFailed: 2,
  playbackStopped: 2,
  bufferEvents: 4,
  bufferTotalMs: 355,
  bufferCurrentMs: 20,
  lastLoadMs: 110,
  lastPlaybackRestartMs: 145,
  libmpvReleases: 8,
  lastReleaseMs: 7,
  maxReleaseMs: 17,
  lastOutcome: null,
};

test("native read-only health contract is bounded to safe scalar values", () => {
  assert.equal(validRuntimeHealth(report), true);
  assert.equal(validRuntimeHealth(null), false);
  assert.equal(validRuntimeHealth(undefined), false);
  assert.equal(validRuntimeHealth({ ...report, schemaVersion: 2 }), false);
  assert.equal(validRuntimeHealth({ ...report, bufferTotalMs: NaN }), false);
  assert.equal(validRuntimeHealth({ ...report, activeSessionId: "7" }), false);
  assert.equal(validRuntimeHealth({ ...report, sessionsOpened: -1 }), false);
  assert.equal(validRuntimeHealth({ ...report, sessionsOpened: 0.5 }), false);
  assert.equal(validRuntimeHealth({ ...report, lastOutcome: "secret" }), false);
});

test("diagnostics disclose no untrusted source metadata and do not imply real first-frame latency", () => {
  const text = formatRuntimeHealth(report);
  assert.match(text, /累计缓冲: 355 ms/);
  assert.match(text, /最大释放耗时: 17 ms/);
  assert.match(text, /不是实际首帧显示时间/);
  assert.doesNotMatch(text, /https?:|filePath|secret|token|mp4/);
  assert.equal(formatRuntimeHealth({ ...report, activeSessionId: null, lastLoadMs: null }).includes("当前会话: —"), true);
});

test("diagnostics use read-only invoke and fail closed on malformed native response", async () => {
  const previous = globalThis.window;
  const calls = [];
  globalThis.window = { __TAURI__: { core: { async invoke(name) {
    calls.push(name);
    return report;
  } } } };
  try {
    assert.deepEqual(await getRuntimeHealth(), report);
    assert.deepEqual(calls, ["get_runtime_health"]);
    globalThis.window.__TAURI__.core.invoke = async () => ({ ...report, lastReleaseMs: -10 });
    await assert.rejects(getRuntimeHealth(), /诊断数据不受支持/);
  } finally {
    globalThis.window = previous;
  }
});
