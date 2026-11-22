import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createCommunityGateway, enrichCatalogCovers, loadSeriesDetail, classifySdkError, convertHeicToJpeg } from "../scripts/hg2a-sdk/server.mjs";

const token = "0123456789abcdef".repeat(4);
const fakeSdk = {
  async discover() {
    return { items: [{ id: "12345", title: "合成目录作品", episode_count: 2, category: "测试", cover_url: "https://p3-reading-sign.fqnovelpic.com/poster.webp?sig=test123" }] };
  },
  async search(query) {
    return { items: query.includes("测试") ? [{ id: "12345", title: "合成搜索作品", episode_count: 2, cover_url: "https://p3-reading-sign.fqnovelpic.com/poster.webp?sig=test123" }] : [] };
  },
  async getSeries(id) {
    return { id, title: "合成测试作品", episode_count: 2, category: "测试" };
  },
  async listEpisodes() {
    return { items: [
      { id: "54321", number: 2, title: "第二集", playable: false },
      { id: "54320", number: 1, title: "第一集", playable: true },
    ] };
  },
  async resolvePlayback() {
    return { playback: { protected: false, streams: [
      { url: "https://samplelib.com/lib/preview/mp4/sample-20s.mp4", protected: false, hls: false, dash: false },
    ] } };
  },
};

async function withServer(sdk, run, options = {}) {
  const server = createCommunityGateway({ sdk, token, ...options });
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

function getRaw(port, path, { origin = null, auth = null } = {}) {
  return new Promise((resolve, reject) => {
    const headers = {};
    if (auth) headers["X-HG2A-Token"] = auth;
    if (origin) headers.Origin = origin;
    http.get({ hostname: "127.0.0.1", port, path, headers, timeout: 2000 }, res => {
      const chunks = [];
      res.on("data", b => chunks.push(b));
      res.on("end", () => resolve({
        code: res.statusCode,
        headers: res.headers,
        body: Buffer.concat(chunks),
      }));
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
    assert.match(discover.body.items[0].coverUrl, new RegExp("^http://127\\.0\\.0\\.1:" + port + "/v1/covers/[a-f0-9]{32}$"));
    assert.doesNotMatch(JSON.stringify(discover.body), /fqnovelpic|sig=test123/);
    const search = await get(port, "/v1/search?q=" + encodeURIComponent("测试"));
    assert.equal(search.body.items[0].title, "合成搜索作品");
    const detail = await get(port, "/v1/series/12345");
    assert.equal(detail.code, 200);
    assert.deepEqual(detail.body.detail.episodes.map(e => e.number), [1, 2]);
    assert.deepEqual(detail.body.detail.episodes.map(e => e.playable), [true, false]);
    const target = await get(port, "/v1/series/12345/episodes/54320/play");
    assert.equal(target.code, 200);
    assert.match(target.body.mediaUrl, /^https:\/\/samplelib.com\//);
  });
});

test("HG-2B real SDK search parser missing cover_url is enriched through one metadata batch", async () => {
  let calls = 0;
  let idsSeen = [];
  const sdk = {
    ...fakeSdk,
    async search() {
      return { items: [
        { id: "765401", title: "真实样式作品一", episode_count: 30 },
        { id: "765402", title: "真实样式作品二", episode_count: 40 },
      ] };
    },
    async getSeriesBatch(ids) {
      calls++;
      idsSeen = ids;
      // Actual upstream getSeriesBatch shape is Array<{series, episodes}>.
      return ids.map(id => ({
        series: { id, title: "from detail, not a new search entry",
          cover_url: "https://p3-reading-sign.fqnovelpic.com/" + id + ".webp?x-expires=1892345678" },
        episodes: [],
      }));
    },
  };
  await withServer(sdk, async port => {
    const reply = await get(port, "/v1/search?q=" + encodeURIComponent("重生"));
    assert.equal(reply.code, 200);
    assert.equal(reply.body.items[0].title, "真实样式作品一");
    assert.match(reply.body.items[0].coverUrl, /^http:\/\/127\.0\.0\.1:\d+\/v1\/covers\/[a-f0-9]{32}$/);
    assert.match(reply.body.items[1].coverUrl, /^http:\/\/127\.0\.0\.1:\d+\/v1\/covers\/[a-f0-9]{32}$/);
    assert.doesNotMatch(JSON.stringify(reply.body), /fqnovelpic|x-expires/);
  });
  assert.equal(calls, 1);
  assert.deepEqual(idsSeen, ["765401", "765402"]);
});

test("HG-2B optional cover batch failure never hides or fabricates search records", async () => {
  const sdk = {
    ...fakeSdk,
    async search() {
      return { items: [{ id: "923456", title: "真实目录元数据原样保留", episode_count: 51 }] };
    },
    async getSeriesBatch() {
      const err = new Error("https://secret.example/private?token=supersecret");
      err.code = "RATE_LIMITED";
      throw err;
    },
  };
  await withServer(sdk, async port => {
    const reply = await get(port, "/v1/search?q=" + encodeURIComponent("重生"));
    assert.equal(reply.code, 200);
    assert.equal(reply.body.items.length, 1);
    assert.equal(reply.body.items[0].title, "真实目录元数据原样保留");
    assert.equal(reply.body.items[0].coverUrl, null);
    assert.doesNotMatch(JSON.stringify(reply.body), /supersecret|secret\.example/);
  });
});

test("HG-2B only one bounded metadata lookup, no spoofed IDs or overwrite of original covers", async () => {
  let batches = 0;
  let requested = [];
  const input = Array.from({ length: 25 }, (_, n) => ({
    id: String(760000 + n), title: "synthetic " + n,
    cover_url: n === 0 ? "https://direct.example.org/direct.webp" : undefined,
  }));
  const sdk = {
    async getSeriesBatch(ids) {
      batches++;
      requested = ids;
      return [
        { series: { id: "999999", cover_url: "https://evil.example.org/not-requested.jpg" } },
        ...ids.map(id => ({ series: { id, cover_url: "https://detail.example.org/" + id + ".webp" } })),
      ];
    },
  };
  const { rows, diagnostics } = await enrichCatalogCovers(sdk, input);
  assert.equal(batches, 1);
  assert.equal(requested.length, 10);
  assert.equal(rows[0].cover_url, "https://direct.example.org/direct.webp");
  assert.equal(rows[1].cover_url, "https://detail.example.org/760001.webp");
  assert.equal(rows[11].cover_url, undefined);
  assert.equal(diagnostics.direct, 1);
  assert.equal(diagnostics.recovered, 10);
  assert.equal(diagnostics.missing, 14);
  assert.equal(diagnostics.batchState, "ok");
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
    assert.equal((await get(port, "/v1/series/12345/episodes/54321/play")).code, 404);
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

test("HG-2B detail prefers a single SDK batch for public metadata and episodes", async () => {
  let batches = 0;
  const sdk = {
    async getSeriesBatch(ids) {
      batches++;
      assert.deepEqual(ids, ["12345"]);
      return [{ series: { id: "12345", title: "批量详情", episode_count: 2,
        cover_url: "https://p3-reading-sign.fqnovelpic.com/12345.heic?sig=detail" }, episodes: [
        { id: "54320", number: 1, title: "第1集", playable: true },
        { id: "54321", number: 2, title: "第2集", playable: false },
      ] }];
    },
    async getSeries() { throw Error("must not request separate series metadata"); },
    async listEpisodes() { throw Error("must not request separate episode list"); },
  };
  await withServer(sdk, async port => {
    const response = await get(port, "/v1/series/12345");
    assert.equal(response.code, 200);
    assert.match(response.body.detail.series.coverUrl, /^http:\/\/127\.0\.0\.1:\d+\/v1\/covers\/[a-f0-9]{32}$/);
    assert.deepEqual(response.body.detail.episodes.map(x => x.playable), [true, false]);
  });
  assert.equal(batches, 1);
});

test("HG-2B detail rejects wrong batch IDs without accidental cross-title metadata", async () => {
  const sdk = { getSeriesBatch: async () => [
    { series: { id: "another", title: "Other" }, episodes: [{ id: "1" }] },
  ] };
  assert.equal(await loadSeriesDetail(sdk, "12345"), null);
  await withServer(sdk, async port => {
    const response = await get(port, "/v1/series/12345");
    assert.equal(response.code, 404);
    assert.equal(response.body.error, "not_found");
  });
});

test("HG-2B SDK error categorization is bounded and never leaks raw exception text", async () => {
  assert.deepEqual(classifySdkError({ code: "TRANSPORT", message: "token=secret" }),
    { code: "TRANSPORT", status: 503 });
  assert.deepEqual(classifySdkError({ code: "RATE_LIMITED", message: "secret" }),
    { code: "RATE_LIMITED", status: 429 });
  assert.deepEqual(classifySdkError({ code: "CONTENT_UNAVAILABLE" }),
    { code: "CONTENT_UNAVAILABLE", status: 404 });
  assert.deepEqual(classifySdkError({ code: "secret=https://private.invalid/x" }),
    { code: "UPSTREAM_UNAVAILABLE", status: 503 });
  const sdk = { getSeriesBatch: async () => {
    const error = Error("https://private.invalid/cover.jpg?token=do-not-log");
    error.code = "TRANSPORT";
    throw error;
  } };
  await withServer(sdk, async port => {
    const response = await get(port, "/v1/series/12345");
    assert.equal(response.code, 503);
    assert.deepEqual(response.body, { error: "TRANSPORT" });
    assert.doesNotMatch(JSON.stringify(response.body), /private|token|do-not-log/);
  });
});


test("HG-2B HEIC covers are served as local JPEG without exposing signed CDN URLs", async () => {
  let fetchedUrl = null;
  await withServer(fakeSdk, async port => {
    const catalog = await get(port, "/v1/search?q=");
    const coverUrl = catalog.body.items[0].coverUrl;
    const local = new URL(coverUrl);
    assert.equal(local.origin, "http://127.0.0.1:" + port);
    assert.match(local.pathname, /^\/v1\/covers\/[a-f0-9]{32}$/);
    assert.doesNotMatch(JSON.stringify(catalog.body), /fqnovelpic|sig=test123/);

    // A browser image request needs no SDK header; the random per-launch ID
    // grants read-only access to this single poster.
    const image = await getRaw(port, local.pathname, { origin: "http://tauri.localhost" });
    assert.equal(image.code, 200);
    assert.equal(image.headers["content-type"], "image/jpeg");
    assert.equal(image.body.toString("utf8"), "jpeg-poster");
    const denied = await getRaw(port, local.pathname, { origin: "https://evil.example" });
    assert.equal(denied.code, 403);
  }, {
    fetchCover: async url => {
      fetchedUrl = url;
      return { contentType: "image/heic", body: Buffer.from("synthetic-heic") };
    },
    convertHeic: async input => {
      assert.equal(input.toString("utf8"), "synthetic-heic");
      return Buffer.from("jpeg-poster");
    },
  });
  assert.equal(fetchedUrl, "https://p3-reading-sign.fqnovelpic.com/poster.webp?sig=test123");
});

test("HG-2B cover relay refuses unapproved hosts and unknown opaque IDs", async () => {
  const sdk = {
    async search() {
      return { items: [{ id: "12345", title: "host filter", episode_count: 1,
        cover_url: "https://untrusted.example.org/poster.heic?sig=must-not-leak" }] };
    },
  };
  await withServer(sdk, async port => {
    const result = await get(port, "/v1/search?q=host");
    assert.equal(result.code, 200);
    assert.equal(result.body.items[0].coverUrl, null);
    assert.doesNotMatch(JSON.stringify(result.body), /untrusted|must-not-leak/);
    const unknown = await getRaw(port, "/v1/covers/0123456789abcdef0123456789abcdef");
    assert.equal(unknown.code, 404);
  });
});

test("HG-2B HEIC decoder dependencies load and malformed input fails closed", async () => {
  await assert.rejects(convertHeicToJpeg(Buffer.from("not-a-heic-file")), /cover_decode_failed/);
});
