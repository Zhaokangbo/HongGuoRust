import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createCommunityGateway } from "../scripts/hg2a-sdk/server.mjs";

const token = "0123456789abcdef".repeat(4);
const fakeSdk = {
  async discover() {
    return { items: [{ id: "12345", title: "合成目录作品", episode_count: 2, category: "测试" }] };
  },
  async search(query) {
    return { items: query.includes("测试") ? [{ id: "12345", title: "合成搜索作品", episode_count: 2 }] : [] };
  },
  async getSeries(id) {
    return { id, title: "合成测试作品", episode_count: 2, category: "测试" };
  },
  async listEpisodes() {
    return { items: [
      { id: "54321", number: 2, title: "第二集", playable: true },
      { id: "54320", number: 1, title: "第一集", playable: true },
    ] };
  },
  async resolvePlayback() {
    return { playback: { protected: false, streams: [
      { url: "https://samplelib.com/lib/preview/mp4/sample-20s.mp4", protected: false, hls: false, dash: false },
    ] } };
  },
};

async function withServer(sdk, run) {
  const server = createCommunityGateway({ sdk, token });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  try {
    await run(server.address().port);
  } finally {
    await new Promise((resolve, reject) => server.close(err => err ? reject(err) : resolve()));
  }
}

function get(port, path, auth = token, origin = null) {
  return new Promise((resolve, reject) => {
    const headers = { "X-HG2A-Token": auth };
    if (origin) headers.Origin = origin;
    http.get({ hostname: "127.0.0.1", port, path, headers, timeout: 2000 }, res => {
      const chunks = [];
      res.on("data", b => chunks.push(b));
      res.on("end", () => {
        try {
          resolve({ code: res.statusCode, body: JSON.parse(Buffer.concat(chunks).toString()) });
        } catch (err) { reject(err); }
      });
    }).on("error", reject);
  });
}

test("real-SDK-shaped adapter maps catalog/search/detail into fixed bounded JSON", async () => {
  await withServer(fakeSdk, async port => {
    const health = await get(port, "/health");
    assert.equal(health.code, 200);
    assert.equal(health.body.official, false);
    const discover = await get(port, "/v1/search?q=");
    assert.equal(discover.code, 200);
    assert.equal(discover.body.items[0].id, "12345");
    const search = await get(port, "/v1/search?q=" + encodeURIComponent("测试"));
    assert.equal(search.body.items[0].title, "合成搜索作品");
    const detail = await get(port, "/v1/series/12345");
    assert.equal(detail.code, 200);
    assert.deepEqual(detail.body.detail.episodes.map(e => e.number), [1, 2]);
    const target = await get(port, "/v1/series/12345/episodes/54320/play");
    assert.equal(target.code, 200);
    assert.match(target.body.mediaUrl, /^https:\/\/samplelib.com\//);
  });
});

test("missing or wrong local token and cross-site Origin are blocked before SDK runs", async () => {
  let invoked = 0;
  const sdk = { ...fakeSdk, async search() { invoked++; return { items: [] }; } };
  await withServer(sdk, async port => {
    assert.equal((await get(port, "/v1/search?q=test", "bad")).code, 403);
    assert.equal((await get(port, "/v1/search?q=test", token, "https://evil.example")).code, 403);
    assert.equal(invoked, 0);
  });
});

test("protected streams are not decrypted or returned as playable targets", async () => {
  const sdk = { ...fakeSdk, async resolvePlayback() {
    return { playback: { protected: true, streams: [
      { url: "https://protected.example.com/media.mp4", protected: true },
    ] }, contentKey: { key: "do-not-expose" } };
  } };
  await withServer(sdk, async port => {
    const blocked = await get(port, "/v1/series/12345/episodes/54320/play");
    assert.equal(blocked.code, 403);
    assert.equal(blocked.body.error, "protected_media_not_supported");
    assert.doesNotMatch(JSON.stringify(blocked.body), /do-not-expose|protected\.example/);
  });
});

test("upstream errors produce bounded status and no raw token/path leakage", async () => {
  const sdk = { ...fakeSdk, async search() {
    const err = new Error("https://private.example/movie.mp4?token=secret");
    err.code = "RATE_LIMITED";
    throw err;
  } };
  await withServer(sdk, async port => {
    const result = await get(port, "/v1/search?q=sensitive");
    assert.equal(result.code, 429);
    assert.deepEqual(result.body, { error: "RATE_LIMITED" });
  });
});

test("invalid identifiers and disabled episodes cannot resolve targets", async () => {
  await withServer(fakeSdk, async port => {
    assert.equal((await get(port, "/v1/series/%2E%2E%2Fprivate")).code, 404);
    assert.equal((await get(port, "/v1/series/12345/episodes/99999/play")).code, 404);
    assert.equal((await get(port, "/v1/search?q=" + "z".repeat(200))).code, 400);
  });
});

test("direct Windows-compatible CLI invocation fails closed without explicit opt-in", () => {
  const script = fileURLToPath(new URL("../scripts/hg2a-sdk/server.mjs", import.meta.url));
  const result = spawnSync(process.execPath, [script], {
    env: { ...process.env, HONGGUORUST_HG2A_EXPERIMENT: "0", HONGGUORUST_HG2A_TOKEN: "" },
    encoding: "utf8",
    timeout: 5_000,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 1, "direct invocation must execute the main() opt-in gate");
  assert.match(result.stderr, /HG2A startup failed/);
  assert.doesNotMatch(result.stderr, /token=|anonymous\.json|\.mp4\?/);
});
