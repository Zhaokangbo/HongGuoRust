import test from "node:test";
import assert from "node:assert/strict";
import {
  adjacentEpisode, createEpisodeQueue, currentEpisodeEnded, currentEpisodeFailed,
} from "../src/player/episodeQueue.ts";

const detail = {
  series: {
    id: "manifest:sample", providerId: "manifest", title: "测试多集",
    description: "", genre: "测试", badge: "", artwork: "studio", episodeCount: 3,
  },
  episodes: [
    { id: "ep-03", number: 3, title: "第三集", runtimeLabel: "MP4" },
    { id: "ep-01", number: 1, title: "第一集", runtimeLabel: "MP4" },
    { id: "ep-02", number: 2, title: "第二集", runtimeLabel: "MP4" },
  ],
};

function snapshot(state, sessionId) {
  return { state, sessionId, revision: 1, appVersion: "test" };
}

test("M10-B preserves scoped series ID and sorts actual numbered episodes", () => {
  const q = createEpisodeQueue(detail, "ep-02", 42);
  assert.equal(q.seriesId, "manifest:sample");
  assert.equal(q.index, 1);
  assert.deepEqual(q.episodes.map(e => e.id), ["ep-01", "ep-02", "ep-03"]);
  assert.deepEqual(detail.episodes.map(e => e.id), ["ep-03", "ep-01", "ep-02"]);
  assert.deepEqual(adjacentEpisode(q, -1), { index: 0, episode: q.episodes[0] });
  assert.deepEqual(adjacentEpisode(q, 1), { index: 2, episode: q.episodes[2] });
});

test("both ends and a single-episode local source disable invalid navigation", () => {
  const q = createEpisodeQueue(detail, "ep-01", 11);
  assert.equal(adjacentEpisode(q, -1), null);
  const last = createEpisodeQueue(detail, "ep-03", 12);
  assert.equal(adjacentEpisode(last, 1), null);
  const local = createEpisodeQueue({
    ...detail, series: { ...detail.series, id: "local:7", providerId: "local" },
    episodes: [{ id: "ep-01", number: 1, title: "本地 MP4", runtimeLabel: "MP4" }],
  }, "ep-01", 30);
  assert.equal(adjacentEpisode(local, -1), null);
  assert.equal(adjacentEpisode(local, 1), null);
});

test("only a matching native-session natural EOF shows the completion prompt", () => {
  const q = createEpisodeQueue(detail, "ep-02", 55);
  assert.equal(currentEpisodeEnded(q, snapshot("Ended", 55)), true);
  for (const state of ["Idle", "Loading", "Playing", "Paused", "Buffering", "Error"]) {
    assert.equal(currentEpisodeEnded(q, snapshot(state, 55)), false);
  }
  assert.equal(currentEpisodeEnded(q, snapshot("Ended", 54)), false);
  assert.equal(currentEpisodeEnded(q, snapshot("Ended", 56)), false);
  assert.equal(currentEpisodeEnded(null, snapshot("Ended", 55)), false);
  assert.equal(currentEpisodeFailed(q, snapshot("Error", 55)), true);
  assert.equal(currentEpisodeFailed(q, snapshot("Error", 54)), false);
});

test("unknown, duplicate or untrustworthy episode navigation is rejected", () => {
  assert.throws(() => createEpisodeQueue(detail, "ep-99", 50));
  assert.throws(() => createEpisodeQueue(detail, "ep-01", 0));
  assert.throws(() => createEpisodeQueue(detail, "ep-01", -10));
  assert.throws(() => createEpisodeQueue(detail, "ep-01", Number.NaN));
  assert.throws(() => createEpisodeQueue({
    ...detail, episodes: [detail.episodes[0], detail.episodes[0]],
  }, "ep-03", 40));
});
