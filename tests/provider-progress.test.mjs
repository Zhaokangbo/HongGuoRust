import { test } from "node:test";
import assert from "node:assert/strict";
import { latestEpisodeProgress, formatEpisodePosition } from "../src/catalog/progress.ts";

const episodes = [
  { id: "a", number: 1, title: "A", runtimeLabel: "10:00" },
  { id: "b", number: 2, title: "B", runtimeLabel: "12:00" },
];
const item = (episodeId, lastPlayedMs, positionSec = 30, completed = false) => ({
  seriesId: "manifest:series", episodeId, lastPlayedMs, positionSec,
  durationSec: 120, completed,
});

test("latest bookmark ignores missing episode and picks most recent valid record", () => {
  assert.equal(latestEpisodeProgress(episodes, [
    item("a", 100), item("removed", 900), item("b", 200),
  ])?.episodeId, "b");
  assert.equal(latestEpisodeProgress(episodes, [item("unknown", 1)]), null);
});

test("completed record stays visible for explicit replay", () => {
  assert.equal(latestEpisodeProgress(episodes, [item("b", 200, 120, true)])?.completed, true);
});

test("malformed positions and dates never become a resume choice", () => {
  assert.equal(latestEpisodeProgress(episodes, [item("a", NaN)]), null);
  assert.equal(latestEpisodeProgress(episodes, [item("a", 100, -1)]), null);
});

test("progress UI displays bounded minute-second text", () => {
  assert.equal(formatEpisodePosition(73.8), "01:13");
  assert.equal(formatEpisodePosition(-1), "--:--");
});
