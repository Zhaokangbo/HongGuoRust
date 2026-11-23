/** HG-2A: Synthetic HTTP catalog for Windows integration only.
 * Never proxies external websites; no official HongGuo endpoint, account, token,
 * copyrighted episode data, login, DRM or unapproved media access.
 * Node.js >=22.12, stdlib only; binds exclusively to 127.0.0.1.
 */
import http from "node:http";
import { fileURLToPath } from "node:url";

const FIXTURE_MP4 = "https://samplelib.com/lib/preview/mp4/sample-20s.mp4";
const FIXTURES = Object.freeze([
  { id: "lab-story-one", title: "实验室虚构短剧一", description: "仅用于 HG-2A 本地模拟接口测试", genre: "测试", episodes: 3 },
  { id: "lab-story-two", title: "实验室虚构短剧二", description: "不代表红果平台或任何真实影视作品", genre: "演示", episodes: 2 },
]);
const EPISODE_ID = /^ep-(\d{2})$/;
const SERIES_ID = /^[a-z0-9-]{1,96}$/;

function respond(res, code, value, extraHeaders = {}) {
  const body = Buffer.from(JSON.stringify(value), "utf8");
  res.writeHead(code, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": String(body.length),
    "Cache-Control": "no-store",
    "Connection": "close",
    "X-Mock-Content": "synthetic-not-hongguo",
    ...extraHeaders,
  });
  res.end(body);
}

export function createHg2aMockServer({ fault = "none" } = {}) {
  if (!["none", "error", "redirect", "oversize", "slow"].includes(fault)) {
    throw new Error("Unsupported mock fault mode");
  }
  return http.createServer((req, res) => {
    if (req.method !== "GET") {
      respond(res, 405, { error: "method_not_allowed" });
      return;
    }
    let url;
    try {
      url = new URL(req.url, "http://127.0.0.1");
    } catch {
      respond(res, 400, { error: "invalid_path" });
      return;
    }
    if (url.pathname === "/health") {
      respond(res, 200, { schemaVersion: 1, status: "synthetic", hongguoLicensed: false });
      return;
    }
    const route = () => {
      if (fault === "error") {
        respond(res, 503, { error: "controlled_mock_failure" });
        return;
      }
      if (fault === "redirect") {
        respond(res, 302, { error: "redirect_intentionally_rejected" }, { Location: "https://example.org/elsewhere" });
        return;
      }
      if (fault === "oversize") {
        respond(res, 200, { padding: "x".repeat(200_000) });
        return;
      }
      if (url.pathname === "/v1/search" && [...url.searchParams.keys()].every(k => k === "q")) {
        const q = (url.searchParams.get("q") ?? "").trim().toLowerCase();
        const items = FIXTURES.filter(item =>
          !q || item.title.toLowerCase().includes(q) || item.genre.toLowerCase().includes(q)
        );
        respond(res, 200, { schemaVersion: 1, items });
        return;
      }
      const parts = url.pathname.split("/");
      // Hardcoded routes and limited IDs only; never accept arbitrary paths.
      if (parts[1] === "v1" && parts[2] === "series" &&
          SERIES_ID.test(parts[3] ?? "") && url.search === "") {
        const item = FIXTURES.find(entry => entry.id === parts[3]);
        if (!item) {
          respond(res, 404, { error: "not_found" });
          return;
        }
        if (parts.length === 4) {
          const episodes = Array.from({ length: item.episodes }, (_, i) => ({
            id: "ep-" + String(i + 1).padStart(2, "0"),
            number: i + 1,
            title: "第 " + String(i + 1) + " 集 · 合成测试",
          }));
          respond(res, 200, { schemaVersion: 1, detail: { series: item, episodes } });
          return;
        }
        if (parts.length === 7 && parts[4] === "episodes" && parts[6] === "play") {
          const parsed = EPISODE_ID.exec(parts[5] ?? "");
          const n = parsed ? Number(parsed[1]) : 0;
          if (n < 1 || n > item.episodes) {
            respond(res, 404, { error: "episode_not_found" });
            return;
          }
          respond(res, 200, { schemaVersion: 1, mediaUrl: FIXTURE_MP4 });
          return;
        }
      }
      respond(res, 404, { error: "not_found" });
    };
    if (fault === "slow") {
      // Controlled timeout/recovery test; keep only synthetic responses.
      setTimeout(route, 4000);
    } else {
      route();
    }
  });
}

function parseArgs(args) {
  let port = 19091, fault = "none";
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port" && i + 1 < args.length) {
      const v = args[++i];
      if (!/^\d+$/.test(v) || Number(v) < 1024 || Number(v) > 65535) throw new Error("Invalid port");
      port = Number(v);
    } else if (args[i] === "--fault" && i + 1 < args.length) {
      fault = args[++i];
    } else {
      throw new Error("Usage: node scripts/hg2a-mock-server.mjs [--port 19091] [--fault none|slow|error|redirect|oversize]");
    }
  }
  return { port, fault };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === fileURLToPath(new URL("file://" + process.argv[1].replaceAll("\\", "/")).href)) {
  const { port, fault } = parseArgs(process.argv.slice(2));
  createHg2aMockServer({ fault }).listen(port, "127.0.0.1", () => {
    process.stdout.write("HG-2A synthetic-only mock listening on http://127.0.0.1:" + port + "\n");
    process.stdout.write("No HongGuo service or credential is accessed. Ctrl+C to stop.\n");
  });
}
