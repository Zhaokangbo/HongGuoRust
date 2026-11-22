import { test } from "node:test";
import assert from "node:assert/strict";
import {
  browseLocalMedia, localLibrarySummary, playlistFromVisible,
} from "../src/catalog/localBrowse.ts";

function media(overrides = {}) {
  return {
    id: 1, title: "山海", filename: "shan-hai.mp4", available: true,
    positionSec: 30, durationSec: 100, lastPlayedMs: 300, completed: false, favorite: false,
    ...overrides,
  };
}

const records = [
  media({ id: 1, title: "第10集", filename: "episode-10.mp4", positionSec: 80, lastPlayedMs: 100, favorite: true }),
  media({ id: 2, title: "第2集", filename: "episode-2.mp4", positionSec: 20, lastPlayedMs: 300 }),
  media({ id: 3, title: "已看完", filename: "done.mp4", positionSec: 100, lastPlayedMs: 200, completed: true, favorite: true }),
  media({ id: 4, title: "文件缺失", filename: "missing.mp4", available: false, lastPlayedMs: 400 }),
  media({ id: 5, title: "尚未播放", filename: "new.mp4", lastPlayedMs: null, positionSec: 0 }),
];

test("mode scopes keep local, history and favorites independent", () => {
  assert.equal(browseLocalMedia(records, "local", "", "added").total, 5);
  assert.equal(browseLocalMedia(records, "history", "", "recent").total, 4);
  assert.deepEqual(browseLocalMedia(records, "favorites", "", "added").visible.map(item => item.id), [3, 1]);
});

test("status filters distinguish available, missing, in-progress, completed and unplayed", () => {
  assert.deepEqual(browseLocalMedia(records, "local", "", "added", "available").visible.map(item => item.id), [5, 3, 2, 1]);
  assert.deepEqual(browseLocalMedia(records, "local", "", "added", "missing").visible.map(item => item.id), [4]);
  assert.deepEqual(browseLocalMedia(records, "history", "", "recent", "in-progress").visible.map(item => item.id), [4, 2, 1]);
  assert.deepEqual(browseLocalMedia(records, "history", "", "recent", "completed").visible.map(item => item.id), [3]);
  assert.deepEqual(browseLocalMedia(records, "local", "", "added", "unplayed").visible.map(item => item.id), [5]);
});

test("search matches title or filename and reports status-filter count separately", () => {
  const result = browseLocalMedia(records, "local", "EPISODE", "title", "available");
  assert.equal(result.total, 5);
  assert.equal(result.matched, 4);
  assert.deepEqual(result.visible.map(item => item.id), [2, 1]);
});

test("title sorting uses Chinese natural order and progress sorting puts completed first", () => {
  assert.deepEqual(browseLocalMedia(records, "local", "", "title").visible.slice(0, 2).map(item => item.id), [2, 1]);
  assert.deepEqual(browseLocalMedia(records, "local", "", "progress").visible.map(item => item.id), [3, 1, 4, 2, 5]);
});

test("summary counts describe the active mode without losing global favorite count", () => {
  assert.deepEqual(localLibrarySummary(records, "local"), {
    total: 5, available: 4, missing: 1, inProgress: 3, completed: 1, favorites: 2,
  });
  assert.deepEqual(localLibrarySummary(records, "history"), {
    total: 4, available: 3, missing: 1, inProgress: 3, completed: 1, favorites: 2,
  });
  assert.deepEqual(localLibrarySummary(records, "favorites"), {
    total: 2, available: 2, missing: 0, inProgress: 1, completed: 1, favorites: 2,
  });
});

test("playback queue preserves visible order and excludes unavailable files", () => {
  const visible = browseLocalMedia(records, "local", "", "added").visible;
  assert.deepEqual(playlistFromVisible(visible), [5, 3, 2, 1]);
  assert.deepEqual(playlistFromVisible([records[3], records[0], records[1]]), [1, 2]);
});

test("view derivation never mutates the persisted source array", () => {
  const input = [...records];
  const before = input.map(item => item.id);
  browseLocalMedia(input, "local", "第", "title", "available");
  assert.deepEqual(input.map(item => item.id), before);
});
