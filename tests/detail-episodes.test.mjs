import { test } from "node:test";
import assert from "node:assert/strict";
import { filterEpisodes, summarizeEpisodes } from "../src/catalog/detailEpisodes.ts";

const episodes = [
  { id: "e3", number: 3, title: "第三集：重逢", runtimeLabel: "45:00" },
  { id: "e1", number: 1, title: "第一集：序章", runtimeLabel: "44:00" },
  { id: "e2", number: 2, title: "第二集：雨夜", runtimeLabel: "46:00" },
  { id: "e4", number: 4, title: "第四集：终章", runtimeLabel: "45:00", playable: false },
];
const progress = [
  { seriesId: "series-a", episodeId: "e1", positionSec: 2700, durationSec: 2700, lastPlayedMs: 100, completed: true },
  { seriesId: "series-a", episodeId: "e2", positionSec: 600, durationSec: 2700, lastPlayedMs: 200, completed: false },
  { seriesId: "another-series", episodeId: "e3", positionSec: 2000, durationSec: 2700, lastPlayedMs: 300, completed: true },
];

test("episode search matches title and number while ignoring whitespace", () => {
  assert.deepEqual(filterEpisodes(episodes, [], "series-a", "  第 2 集 ", "all", "ascending")
    .map(row => row.episode.id), ["e2"]);
  assert.deepEqual(filterEpisodes(episodes, [], "series-a", "重 逢", "all", "ascending")
    .map(row => row.episode.id), ["e3"]);
});

test("episode filters distinguish incomplete, completed and upstream unavailable", () => {
  assert.deepEqual(filterEpisodes(episodes, progress, "series-a", "", "completed", "ascending")
    .map(row => row.episode.id), ["e1"]);
  assert.deepEqual(filterEpisodes(episodes, progress, "series-a", "", "incomplete", "ascending")
    .map(row => row.episode.id), ["e2", "e3", "e4"]);
  assert.deepEqual(filterEpisodes(episodes, progress, "series-a", "", "unavailable", "ascending")
    .map(row => row.episode.id), ["e4"]);
});

test("episode list supports descending order without mutating source arrays", () => {
  const before = episodes.map(item => item.id);
  assert.deepEqual(filterEpisodes(episodes, [], "series-a", "", "all", "descending")
    .map(row => row.episode.number), [4, 3, 2, 1]);
  assert.deepEqual(episodes.map(item => item.id), before);
});

test("progress is scoped to the selected series and exposes safe percentages", () => {
  const rows = filterEpisodes(episodes, progress, "series-a", "", "all", "ascending");
  assert.equal(rows[0].percent, 100);
  assert.equal(rows[1].percent, 22);
  assert.equal(rows[2].progress, null);
  assert.equal(rows[3].progress, null);
});

test("episode summary reports completed, in-progress and unavailable counts", () => {
  assert.deepEqual(summarizeEpisodes(episodes, progress, "series-a"), {
    total: 4, completed: 1, inProgress: 1, unavailable: 1, percent: 25,
  });
  assert.deepEqual(summarizeEpisodes([], [], "series-a"), {
    total: 0, completed: 0, inProgress: 0, unavailable: 0, percent: 0,
  });
});
