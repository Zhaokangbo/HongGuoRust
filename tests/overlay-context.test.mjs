import test from "node:test";
import assert from "node:assert/strict";
import { isOverlayContext } from "../src/player/overlayContext.ts";
import { isVisibleProvider, visibleCatalogSeries } from "../src/catalog/discovery.ts";

test("overlay context validator accepts series, local and single payloads only", () => {
  assert.equal(isOverlayContext({
    kind: "series",
    seriesId: "manifest:s1",
    seriesTitle: "剧名",
    episodes: [{ id: "ep-1", number: 1, title: "第一集", runtimeLabel: "02:00", playable: true }],
    index: 0,
    sessionId: 7,
  }), true);

  assert.equal(isOverlayContext({
    kind: "local",
    ids: [1, 2],
    titles: { "1": "A" },
    index: 1,
    origin: "本地片库",
    sessionId: 9,
  }), true);

  assert.equal(isOverlayContext({ kind: "single", title: "本地视频", sessionId: 3 }), true);

  assert.equal(isOverlayContext(null), false);
  assert.equal(isOverlayContext({ kind: "single", title: "bad", sessionId: 0 }), false);
  assert.equal(isOverlayContext({
    kind: "series",
    seriesId: "manifest:s1",
    seriesTitle: "剧名",
    episodes: [],
    index: 0,
    sessionId: 1,
  }), false);
});

test("test and placeholder providers are hidden from production browse surfaces", () => {
  assert.equal(isVisibleProvider("demo"), false);
  assert.equal(isVisibleProvider("hongguo"), false);
  assert.equal(isVisibleProvider("local"), true);
  assert.equal(isVisibleProvider("manifest"), true);
  assert.equal(isVisibleProvider("hgcommunity"), true);

  const filtered = visibleCatalogSeries([
    { id: "demo:a", providerId: "demo", title: "A", description: "", genre: "", badge: "", artwork: "forest", episodeCount: 1 },
    { id: "local:1", providerId: "local", title: "B", description: "", genre: "", badge: "", artwork: "studio", episodeCount: 1 },
    { id: "manifest:m1", providerId: "manifest", title: "C", description: "", genre: "", badge: "", artwork: "city", episodeCount: 2 },
  ]);
  assert.deepEqual(filtered.map(item => item.id), ["local:1", "manifest:m1"]);
});
