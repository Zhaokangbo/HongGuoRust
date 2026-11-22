// HG-2A experimental REAL upstream adapter using openhongguo 0.4.0.
// This is a nonofficial, user-opt-in local prototype, NOT a licensed API.
// Never returns device profiles or content keys. Opt-in unprotected MP4 URLs
// may be signed; they are only returned over authenticated local loopback
// and must not be printed in ordinary diagnostics.
import http from "node:http";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const PORT = 19091;
const idPattern = /^[a-zA-Z0-9_-]{1,96}$/;
const safeText = (v, n) => typeof v === "string" ? v.replace(/[\x00-\x1f\x7f]/g, " ").slice(0, n) : "";
/** Validate URL syntax before a WebView loads an image. This does not
 * establish licensing or guarantee safe DNS/redirect behavior. */
export function publicCoverUrl(raw) {
  if (typeof raw !== "string" || raw.length === 0 || raw.length > 2048 ||
      /[\x00-\x20\x7f]/.test(raw) || raw.includes("#")) return null;
  let u;
  try { u = new URL(raw); }
  catch { return null; }
  const host = u.hostname.toLowerCase();
  const suffix = host.split(".").at(-1) ?? "";
  if (u.protocol !== "https:" || u.username || u.password || u.port ||
      !/^[a-z0-9.-]+$/.test(host) || host.includes("..") ||
      host.startsWith(".") || host.endsWith(".") ||
      !/^[a-z]{2,}$/.test(suffix) ||
      ["local", "localhost", "internal", "test", "invalid"].includes(suffix) ||
      /^\d+(?:\.\d+){3}$/.test(host)) return null;
  // Keep the unmodified URL: CDN signatures sometimes bind the original query.
  return raw;
}
const series = raw => ({
  id: String(raw.id || ""),
  title: safeText(raw.title, 100),
  description: safeText(raw.description || "来自社区非官方 SDK 的实验目录", 300),
  coverUrl: publicCoverUrl(raw.cover_url),
  genre: safeText(raw.category || "短剧", 40),
  episodes: Math.min(200, Math.max(1, Number(raw.episode_count) || 1)),
});
function reply(res, status, value) {
  let data = Buffer.from(JSON.stringify(value));
  if (data.length > 96 * 1024) {
    status = 502;
    data = Buffer.from('{"error":"response_too_large"}');
  }
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": String(data.length),
    "Cache-Control": "no-store",
    "Connection": "close",
    "X-Content-Type-Options": "nosniff",
  });
  res.end(data);
}
async function route(sdk, url) {
  if (url.pathname === "/health" && !url.search) {
    return [200, { schemaVersion: 1, status: "unofficial-community-experimental", official: false }];
  }
  if (url.pathname === "/v1/search" && [...url.searchParams.keys()].every(k => k === "q")) {
    const query = url.searchParams.get("q") || "";
    if (Buffer.byteLength(query) > 128 || /[\x00-\x1f\x7f]/.test(query)) {
      return [400, { error: "invalid_query" }];
    }
    const page = query.trim()
      ? await sdk.search(query.trim(), { pageSize: 20, filters: ["content_type_short_play"] })
      : await sdk.discover({});
    const items = (page.items || []).slice(0, 50).filter(x => idPattern.test(x.id || "")).map(series);
    return [200, { schemaVersion: 1, items }];
  }
  const parts = url.pathname.split("/");
  if (url.search || parts[1] !== "v1" || parts[2] !== "series" || !idPattern.test(parts[3] || "")) {
    return [404, { error: "not_found" }];
  }
  const seriesId = parts[3];
  if (parts.length === 4) {
    const [raw, page] = await Promise.all([
      sdk.getSeries(seriesId), sdk.listEpisodes(seriesId, { pageSize: 200 }),
    ]);
    const ids = new Set();
    const episodes = (page.items || []).slice(0, 200)
      .filter(e => idPattern.test(e.id || "") && !ids.has(e.id) && ids.add(e.id))
      .map((e, i) => ({
        id: e.id, number: Number.isSafeInteger(e.number) && e.number > 0 ? e.number : i + 1,
        playable: e.playable === true,
        title: safeText(e.title || "第 " + (i + 1) + " 集", 100),
      })).filter(e => e.number <= 200).sort((a, b) => a.number - b.number);
    if (!episodes.length) return [404, { error: "no_available_episodes" }];
    const info = series({ ...raw, id: seriesId, episode_count: episodes.length });
    return [200, { schemaVersion: 1, detail: { series: info, episodes } }];
  }
  if (parts.length === 7 && parts[4] === "episodes" && idPattern.test(parts[5] || "") && parts[6] === "play") {
    // Do not use upstream contentKey or CENC/DRM material.
    const page = await sdk.listEpisodes(seriesId, { pageSize: 200 });
    if (!(page.items || []).some(e => e.id === parts[5] && e.playable === true)) {
      return [404, { error: "episode_not_found" }];
    }
    const result = await sdk.resolvePlayback(parts[5]);
    if (!result.playback || result.playback.protected) return [403, { error: "protected_media_not_supported" }];
    const stream = (result.playback.streams || []).find(s =>
      !s.protected && !s.hls && !s.dash && typeof s.url === "string" &&
      /^https:\/\/[a-z0-9.-]+(?::443)?\/[^?#]*\.mp4(?:\?[^#]*)?$/i.test(s.url));
    if (!stream) return [422, { error: "no_direct_mp4" }];
    return [200, { schemaVersion: 1, mediaUrl: stream.url }];
  }
  return [404, { error: "not_found" }];
}
export function createCommunityGateway({ sdk, token }) {
  if (!sdk || !/^[a-f0-9]{48,128}$/i.test(token || "")) throw Error("Strong local token required");
  return http.createServer(async (req, res) => {
    if (req.socket.remoteAddress !== "127.0.0.1" ||
        req.headers.host !== "127.0.0.1:" + req.socket.localPort ||
        req.method !== "GET" || req.headers.origin ||
        req.headers["x-hg2a-token"] !== token) {
      reply(res, 403, { error: "local_session_required" }); return;
    }
    let url;
    try { url = new URL(req.url, "http://127.0.0.1"); }
    catch { reply(res, 400, { error: "invalid_route" }); return; }
    if (req.url.length > 1024) { reply(res, 400, { error: "invalid_route" }); return; }
    try {
      const [status, body] = await route(sdk, url);
      reply(res, status, body);
    } catch (error) {
      const known = ["RATE_LIMITED", "ANONYMOUS_LIMITED", "AUTHENTICATION_REQUIRED", "CHALLENGE_REQUIRED", "SIGNATURE_REJECTED", "DEVICE_REJECTED"];
      const code = known.includes(error?.code) ? error.code : "UPSTREAM_UNAVAILABLE";
      reply(res, code === "RATE_LIMITED" ? 429 : 503, { error: code });
    }
  });
}
async function main() {
  if (process.env.HONGGUORUST_HG2A_EXPERIMENT !== "1") throw Error("Explicit opt-in required");
  const token = process.env.HONGGUORUST_HG2A_TOKEN || "";
  if (!/^[a-f0-9]{48,128}$/i.test(token)) throw Error("Random HG2A local token required");
  const { createClient, memoryProfileStore } = await import("openhongguo");
  const sdk = await createClient({
    profile: memoryProfileStore(), retryAttempts: 0, deviceResets: 0, timeoutMs: 8000,
  });
  const server = createCommunityGateway({ sdk, token });
  server.on("error", (error) => {
    // Port conflicts can happen after the PowerShell preflight check.
    // Do not show a stack trace or silently reuse an earlier session/token.
    process.stderr.write(error?.code === "EADDRINUSE"
      ? "HG2A_PORT_IN_USE: 127.0.0.1:19091 is already occupied. Inspect the listener PID.\n"
      : "HG2A_LISTEN_FAILED: local SDK bridge could not bind.\n");
    process.exitCode = 1;
  });
  server.listen(PORT, "127.0.0.1", () => {
    process.stdout.write("Nonofficial HG2A SDK bridge listening at 127.0.0.1:" + PORT + "\n");
  });
}
// Resolve both paths with the host OS path implementation: Windows Node uses \\,
// while argv may contain /; the original mixed-separator comparison silently skipped main().
if (process.argv[1] && resolve(fileURLToPath(import.meta.url)).toLowerCase() === resolve(process.argv[1]).toLowerCase()) {
  main().catch(() => {
    process.stderr.write("HG2A startup failed; check opt-in, SDK install and local token\n");
    process.exitCode = 1;
  });
}
