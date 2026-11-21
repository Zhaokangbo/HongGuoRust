import test from "node:test";
import assert from "node:assert/strict";
import {
  discoveryGenres, discoveryHero, discoveryItems, discoverySourceLabel,
} from "../src/catalog/discovery.ts";

function series(id, providerId, genre, title = id) {
  return {
    id, providerId, genre, title, description: "", badge: "",
    artwork: "studio", episodeCount: 1,
  };
}

test("genres follow only actual loaded catalog metadata and deduplicate", () => {
  const rows = [
    series("demo:one", "demo", "自然影像"),
    series("manifest:a", "manifest", "城市记录"),
    series("local:1", "local", "自然影像"),
    series("local:2", "local", "   "),
  ];
  assert.deepEqual(discoveryGenres(rows), ["城市记录", "自然影像"]);
  assert.equal(rows[3].genre, "   ");
  assert.deepEqual(discoveryGenres([]), []);
});

test("genre chips filter the loaded data without changing provider order", () => {
  const rows = [
    series("demo:one", "demo", "城市记录"),
    series("local:1", "local", "自然影像"),
    series("manifest:a", "manifest", "城市记录"),
  ];
  assert.deepEqual(discoveryItems(rows, "城市记录").map(row => row.id), [
    "demo:one", "manifest:a",
  ]);
  assert.deepEqual(discoveryItems(rows, "all").map(row => row.id), [
    "demo:one", "local:1", "manifest:a",
  ]);
  assert.notStrictEqual(discoveryItems(rows, "all"), rows);
  assert.deepEqual(discoveryItems(rows, "not-present"), []);
});

test("homepage focus prefers user-owned sources, not fabricated popularity", () => {
  const rows = [
    series("demo:one", "demo", "纪录"),
    series("local:1", "local", "纪录"),
    series("manifest:one", "manifest", "纪录"),
  ];
  assert.equal(discoveryHero(rows)?.id, "manifest:one");
  assert.equal(discoveryHero(rows.slice(0, 2))?.id, "local:1");
  assert.equal(discoveryHero(rows.slice(0, 1))?.id, "demo:one");
  assert.equal(discoveryHero([]), null);
});

test("source labels derive from capabilities and have safe fallbacks", () => {
  const names = new Map([["demo", "演示内容"], ["manifest", "用户清单"]]);
  assert.equal(discoverySourceLabel("manifest", names), "用户清单");
  assert.equal(discoverySourceLabel("local", names), "本地媒体");
  assert.equal(discoverySourceLabel("unknown", names), "其他来源");
});
