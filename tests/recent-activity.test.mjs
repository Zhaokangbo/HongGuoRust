import { test } from "node:test";
import assert from "node:assert/strict";
import { combineRecentActivity, watchedEpisodeCount, watchedPercent } from "../src/catalog/recentActivity.ts";

const provider = (seriesId, episodeId, lastPlayedMs, completed = false) => ({
  series: { id: seriesId, title: seriesId, description: "", genre: "测试", badge: "", artwork: "studio", episodeCount: 2, providerId: "manifest" },
  episode: { id: episodeId, number: 1, title: "第一集", runtimeLabel: "00:20" },
  progress: { seriesId, episodeId, positionSec: 9, durationSec: 20, lastPlayedMs, completed },
});
const local = (id, lastPlayedMs, available = true) => ({
  id, title: String(id), filename: String(id) + ".mp4", available, positionSec: 10,
  durationSec: 30, lastPlayedMs, completed: false, favorite: false,
});

test("recent shelf merges M7 and Provider records in descending timestamp order", () => {
  const items = combineRecentActivity([
    provider("manifest:a", "ep-1", 100), provider("demo:b", "ep-1", 300),
  ], [local(7, 200)], 6);
  assert.deepEqual(items.map(item => item.key), ["provider:demo:b", "local:7", "provider:manifest:a"]);
});

test("provider series are deduplicated and missing/source-mismatched records discarded", () => {
  const changed = provider("manifest:a", "ep-2", 400);
  changed.progress.seriesId = "manifest:different";
  const items = combineRecentActivity([
    provider("manifest:a", "ep-1", 300),
    provider("manifest:a", "ep-2", 200),
    changed,
  ], [], 6);
  assert.deepEqual(items.map(item => item.key), ["provider:manifest:a"]);
});

test("invalid time values never enter the recent shelf; unavailable local remains visible", () => {
  const items = combineRecentActivity([
    provider("demo:a", "ep-1", NaN),
  ], [local(10, Infinity), local(11, 250, false)], 6);
  assert.equal(items.length, 1);
  assert.equal(items[0].key, "local:11");
  assert.equal(items[0].kind, "local");
  assert.equal(items[0].value.available, false);
});

test("recent shelf limits items without changing data and normalizes no results", () => {
  const entries = [provider("demo:a", "ep-1", 10), provider("demo:b", "ep-1", 20)];
  assert.equal(combineRecentActivity(entries, [], 1).length, 1);
  assert.equal(combineRecentActivity(entries, [], 0).length, 0);
});

test("watched episode markers use real IDs rather than index and count each episode once", () => {
  assert.equal(watchedEpisodeCount(["ep-1", "ep-3"], [
    { episodeId: "ep-1", lastPlayedMs: 200 },
    { episodeId: "ep-1", lastPlayedMs: 300 },
    { episodeId: "ep-2", lastPlayedMs: 250 },
    { episodeId: "ep-3", lastPlayedMs: 0 },
  ]), 1);
});

test("progress bar clamps values and preserves completed state", () => {
  assert.equal(watchedPercent(10, 20, false), 50);
  assert.equal(watchedPercent(21, 20, false), 100);
  assert.equal(watchedPercent(-2, 20, false), null);
  assert.equal(watchedPercent(3, null, false), null);
  assert.equal(watchedPercent(0, null, true), 100);
});
