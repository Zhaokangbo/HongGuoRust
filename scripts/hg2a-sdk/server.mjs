// HG-2A experimental REAL upstream adapter using openhongguo 0.4.0.
// This is a nonofficial, user-opt-in local prototype, NOT a licensed API.
// Never returns device profiles or content keys. Opt-in unprotected MP4 URLs
// may be signed; they are only returned over authenticated local loopback
// and must not be printed in ordinary diagnostics.
import http from "node:http";
import https from "node:https";
import { randomBytes } from "node:crypto";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const require = createRequire(import.meta.url);
const PORT = 19091;
const idPattern = /^[a-zA-Z0-9_-]{1,96}$/;
const safeText = (v, n) => typeof v === "string" ? v.replace(/[\x00-\x1f\x7f]/g, " ").slice(0, n) : "";
/** Validate URL syntax before a WebView loads an image. This does not
 * establish licensing or guarantee safe DNS/redirect behavior. */
export function publicCoverUrl(raw) {
  if (typeof raw !== "string" || raw.length === 0 || raw.length > 2048 ||
      /[\x00-\x20\x7f]/.test(raw) || raw.includes("#") || raw.includes("\\")) return null;
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

const COVER_HOST_SUFFIX = "fqnovelpic.com";
const COVER_ID_PATTERN = /^[a-f0-9]{32}$/;
const MAX_COVER_SOURCE_BYTES = 5 * 1024 * 1024;
const MAX_COVER_OUTPUT_BYTES = 5 * 1024 * 1024;
const MAX_COVER_PIXELS = 12_000_000;
const COVER_TTL_MS = 15 * 60 * 1000;
const COVER_CACHE_TTL_MS = 5 * 60 * 1000;
const COVER_CACHE_MAX_BYTES = 16 * 1024 * 1024;
const COVER_CACHE_MAX_ITEMS = 100;
const COVER_FETCH_TIMEOUT_MS = 8000;
const COVER_IMAGE_TYPES = new Set([
  "image/heic", "image/heif", "image/jpeg", "image/png",
  "image/webp", "image/avif", "image/gif",
]);
const HEIC_IMAGE_TYPES = new Set(["image/heic", "image/heif"]);

/** Only the observed community poster CDN is eligible for the loopback relay.
 * Never turn the bridge into an arbitrary URL proxy.
 */
export function isApprovedCoverUrl(raw) {
  const safe = publicCoverUrl(raw);
  if (!safe) return false;
  const host = new URL(safe).hostname.toLowerCase();
  return host === COVER_HOST_SUFFIX || host.endsWith("." + COVER_HOST_SUFFIX);
}

function createCoverStore() {
  const entries = new Map();
  const prune = () => {
    const now = Date.now();
    for (const [id, entry] of entries) {
      if (entry.expiresAt <= now) entries.delete(id);
    }
    while (entries.size >= 200) entries.delete(entries.keys().next().value);
  };
  return {
    register(raw) {
      if (!isApprovedCoverUrl(raw)) return null;
      prune();
      const id = randomBytes(16).toString("hex");
      entries.set(id, { url: raw, expiresAt: Date.now() + COVER_TTL_MS });
      return id;
    },
    resolve(id) {
      if (!COVER_ID_PATTERN.test(id)) return null;
      const entry = entries.get(id);
      if (!entry || entry.expiresAt <= Date.now()) {
        entries.delete(id);
        return null;
      }
      return entry.url;
    },
  };
}

function proxySettingFor(host) {
  const noProxy = process.env.NO_PROXY || process.env.no_proxy || "";
  const bypass = noProxy.split(",").map(value => value.trim().toLowerCase()).filter(Boolean)
    .some(rule => rule === "*" || host === rule.replace(/^\./, "") ||
      host.endsWith("." + rule.replace(/^\./, "")));
  if (bypass) return null;
  return process.env.HTTPS_PROXY || process.env.https_proxy ||
    process.env.ALL_PROXY || process.env.all_proxy ||
    process.env.HTTP_PROXY || process.env.http_proxy || null;
}

async function createCoverProxyAgent(host) {
  const proxy = proxySettingFor(host);
  if (!proxy) return null;
  let parsed;
  try { parsed = new URL(proxy); }
  catch { throw new Error("cover_proxy_config_invalid"); }
  if (!["http:", "https:"].includes(parsed.protocol) || !parsed.hostname) {
    throw new Error("cover_proxy_config_invalid");
  }
  try {
    const { HttpsProxyAgent } = await import("https-proxy-agent");
    return new HttpsProxyAgent(parsed);
  } catch {
    throw new Error("cover_proxy_unavailable");
  }
}

function requestCoverOnce(url, agent) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (callback, value) => {
      if (settled) return;
      settled = true;
      callback(value);
    };
    const fail = () => finish(reject, new Error("cover_fetch_failed"));
    const request = https.get(url, {
      agent: agent || undefined,
      headers: {
        Accept: "image/heic,image/heif,image/jpeg,image/png,image/webp,image/avif,image/gif",
        "Accept-Encoding": "identity",
        "User-Agent": "HongGuoRust-HG2B-Cover/1.0",
      },
    }, response => {
      const status = Number(response.statusCode || 0);
      const location = response.headers.location;
      const contentType = String(response.headers["content-type"] || "")
        .split(";")[0].trim().toLowerCase();
      if (status >= 300 && status < 400) {
        response.resume();
        finish(resolve, { status, location });
        return;
      }
      if (status !== 200) {
        response.resume();
        finish(resolve, { status, contentType });
        return;
      }
      if (!COVER_IMAGE_TYPES.has(contentType)) {
        response.resume();
        finish(resolve, { status, contentType, unsupported: true });
        return;
      }
      const declaredLength = Number(response.headers["content-length"] || 0);
      if (declaredLength > MAX_COVER_SOURCE_BYTES) {
        response.destroy();
        finish(reject, new Error("cover_too_large"));
        return;
      }
      const chunks = [];
      let total = 0;
      response.on("data", chunk => {
        total += chunk.length;
        if (total > MAX_COVER_SOURCE_BYTES) {
          response.destroy();
          finish(reject, new Error("cover_too_large"));
          return;
        }
        chunks.push(chunk);
      });
      response.on("end", () => finish(resolve, {
        status, contentType, body: Buffer.concat(chunks, total),
      }));
      response.on("aborted", fail);
      response.on("error", fail);
    });
    request.setTimeout(COVER_FETCH_TIMEOUT_MS, () => request.destroy(new Error("timeout")));
    request.on("error", fail);
  });
}

/** Fetch only the fixed poster CDN over HTTPS. Proxy settings are inherited
 * from the user's environment; failures never silently fall back to direct.
 */
export async function fetchPublicCover(raw) {
  if (!isApprovedCoverUrl(raw)) throw new Error("cover_host_not_allowed");
  let current = raw;
  for (let redirect = 0; redirect <= 2; redirect++) {
    const host = new URL(current).hostname.toLowerCase();
    const agent = await createCoverProxyAgent(host);
    let response;
    try {
      response = await requestCoverOnce(current, agent);
    } finally {
      agent?.destroy();
    }
    if (response.status >= 300 && response.status < 400) {
      if (redirect === 2 || typeof response.location !== "string") {
        throw new Error("cover_redirect_rejected");
      }
      const next = new URL(response.location, current).toString();
      if (!isApprovedCoverUrl(next)) throw new Error("cover_redirect_rejected");
      current = next;
      continue;
    }
    if (response.status !== 200) throw new Error("cover_upstream_unavailable");
    if (response.unsupported || !COVER_IMAGE_TYPES.has(response.contentType)) {
      throw new Error("cover_mime_rejected");
    }
    if (!Buffer.isBuffer(response.body) || response.body.length === 0) {
      throw new Error("cover_empty_response");
    }
    return { contentType: response.contentType, body: response.body };
  }
  throw new Error("cover_redirect_rejected");
}

/** Decode a bounded still HEIC image in WASM and encode a browser-safe JPEG.
 * libheif-js is a pure-JS/WASM dependency; no native codec or Windows extension
 * is required. Raw upstream decoder errors are intentionally not propagated.
 */
async function loadHeicDependencies() {
  // libheif-js documents its Node WASM entry as a CommonJS require().
  // Using createRequire also resolves the package's explicit .js subpath
  // under Node ESM, where extensionless subpath imports are not portable.
  const heif = require("libheif-js/wasm-bundle");
  const jpeg = require("jpeg-js");
  if (typeof heif.HeifDecoder !== "function" || typeof jpeg.encode !== "function") {
    throw new Error("decoder_unavailable");
  }
  return { heif, jpeg };
}

export async function heicRuntimeAvailable() {
  try {
    await loadHeicDependencies();
    return true;
  } catch {
    return false;
  }
}

export async function convertHeicToJpeg(input) {
  let image;
  let decoder;
  try {
    const { heif, jpeg } = await loadHeicDependencies();
    decoder = new heif.HeifDecoder();
    const decoded = decoder.decode(Buffer.from(input));
    image = Array.isArray(decoded) ? decoded[0] : null;
    if (!image || typeof image.display !== "function") throw new Error("invalid_heic");
    const width = Number(image.get_width());
    const height = Number(image.get_height());
    if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
        width < 1 || height < 1 || width > 6000 || height > 6000 ||
        width * height > MAX_COVER_PIXELS) {
      throw new Error("cover_dimensions_rejected");
    }
    const displayData = await new Promise((resolve, reject) => {
      const target = {
        data: new Uint8ClampedArray(width * height * 4),
        width,
        height,
      };
      image.display(target, result => result
        ? resolve(result.data || result)
        : reject(new Error("heic_decode_failed")));
    });
    const rgba = displayData?.data || displayData;
    if (!rgba || rgba.length !== width * height * 4) {
      throw new Error("heic_decode_failed");
    }
    const encoded = jpeg.encode({
      data: Buffer.from(rgba), width, height,
    }, 82);
    const output = Buffer.from(encoded?.data || []);
    if (!output.length || output.length > MAX_COVER_OUTPUT_BYTES) {
      throw new Error("jpeg_encode_failed");
    }
    return output;
  } catch {
    throw new Error("cover_decode_failed");
  } finally {
    if (image && typeof image.free === "function") image.free();
    if (decoder && typeof decoder.free === "function") decoder.free();
  }
}

function replyCoverError(res, status, code) {
  const body = Buffer.from(code, "utf8");
  res.writeHead(status, {
    "Content-Type": "text/plain; charset=utf-8",
    "Content-Length": String(body.length),
    "Cache-Control": "no-store",
    "X-Content-Type-Options": "nosniff",
    "Connection": "close",
  });
  res.end(body);
}

function replyCoverImage(res, image) {
  res.writeHead(200, {
    "Content-Type": image.contentType,
    "Content-Length": String(image.body.length),
    "Cache-Control": "private, max-age=300",
    "X-Content-Type-Options": "nosniff",
    "Cross-Origin-Resource-Policy": "cross-origin",
    "Connection": "close",
  });
  res.end(image.body);
}

const series = (raw, covers, coverOrigin) => {
  const coverId = covers.register(raw.cover_url);
  return {
    id: String(raw.id || ""),
    title: safeText(raw.title, 100),
    description: safeText(raw.description || "来自社区非官方 SDK 的实验目录", 300),
    coverUrl: coverId ? `${coverOrigin}/v1/covers/${coverId}` : null,
    genre: safeText(raw.category || "短剧", 40),
    episodes: Math.min(200, Math.max(1, Number(raw.episode_count) || 1)),
  };
};

/**
 * The upstream SDK's search parser can omit cover_url even when the series
 * details contain it. Recover only missing PUBLIC poster metadata through ONE
 * upstream SDK batch call (max 10 IDs). Never request playback, devices, keys,
 * or hidden login paths, and never mask successful search with a batch error.
 */
export async function enrichCatalogCovers(sdk, inputRows) {
  const rows = Array.isArray(inputRows) ? inputRows : [];
  const direct = rows.filter(row => isApprovedCoverUrl(row?.cover_url)).length;
  const candidates = [...new Set(rows
    .filter(row => idPattern.test(row?.id || "") && !isApprovedCoverUrl(row?.cover_url))
    .map(row => row.id))].slice(0, 10);
  let state = candidates.length ? "unavailable" : "not-needed";
  let recovered = 0;
  let resultRows = rows;

  if (candidates.length && typeof sdk.getSeriesBatch === "function") {
    try {
      const details = await sdk.getSeriesBatch(candidates);
      if (!Array.isArray(details)) throw new Error("invalid_batch_shape");
      const requested = new Set(candidates);
      const coverById = new Map();
      for (const detail of details) {
        const id = detail?.series?.id;
        const cover = isApprovedCoverUrl(detail?.series?.cover_url) ? detail.series.cover_url : null;
        // A mismatched result must never mutate an unrelated search entry.
        if (typeof id === "string" && requested.has(id) && cover) {
          coverById.set(id, cover);
        }
      }
      resultRows = rows.map(row => {
        if (isApprovedCoverUrl(row?.cover_url) || !coverById.has(row?.id)) return row;
        recovered++;
        return { ...row, cover_url: coverById.get(row.id) };
      });
      state = "ok";
    } catch {
      // Suppress upstream errors (possibly carrying URL signatures or device
      // material). A failed optional metadata lookup must not fail search.
      state = "failed";
    }
  }
  return {
    rows: resultRows,
    diagnostics: {
      results: rows.length,
      direct,
      recovered,
      missing: resultRows.length - direct - recovered,
      batchRequested: candidates.length,
      batchState: state,
    },
  };
}

/** Prefer one SDK detail call over concurrent getSeries/listEpisodes requests.
 * The batch endpoint returns both public series metadata and its listed episodes.
 * Never resolve playback here. No retry on upstream rate limit or rejection.
 */
export async function loadSeriesDetail(sdk, seriesId) {
  if (typeof sdk.getSeriesBatch === "function") {
    const result = await sdk.getSeriesBatch([seriesId]);
    if (!Array.isArray(result)) throw Error("invalid_detail_response");
    const entry = result.find(row => row?.series?.id === seriesId && Array.isArray(row?.episodes));
    return entry ? { raw: entry.series, page: { items: entry.episodes } } : null;
  }
  const [raw, page] = await Promise.all([
    sdk.getSeries(seriesId), sdk.listEpisodes(seriesId, { pageSize: 200 }),
  ]);
  return { raw, page };
}

const publicSdkErrorCodes = new Set([
  "RATE_LIMITED", "ANONYMOUS_LIMITED", "AUTHENTICATION_REQUIRED",
  "CHALLENGE_REQUIRED", "SIGNATURE_REJECTED", "DEVICE_REJECTED",
  "CONTENT_UNAVAILABLE", "TRANSPORT", "CANCELLED",
  "SIGNER_UNAVAILABLE", "PARAM_INVALID",
]);
export function classifySdkError(error) {
  const code = publicSdkErrorCodes.has(error?.code) ? error.code : "UPSTREAM_UNAVAILABLE";
  const status = code === "RATE_LIMITED" ? 429 :
    code === "CONTENT_UNAVAILABLE" ? 404 : 503;
  return { code, status };
}

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
async function route(sdk, url, { covers, coverOrigin }) {
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
    const rawItems = (Array.isArray(page?.items) ? page.items : []).slice(0, 50)
      .filter(x => idPattern.test(x?.id || ""));
    const { rows, diagnostics } = await enrichCatalogCovers(sdk, rawItems);
    // Counts only: no source CDN URLs, signatures, titles, device IDs or tokens.
    process.stdout.write("HG2B_COVER_DIAG results=" + diagnostics.results +
      " direct=" + diagnostics.direct + " batch=" + diagnostics.recovered +
      " missing=" + diagnostics.missing + " lookup=" + diagnostics.batchState + "\n");
    const items = rows.map(row => series(row, covers, coverOrigin));
    return [200, { schemaVersion: 1, items }];
  }
  const parts = url.pathname.split("/");
  if (url.search || parts[1] !== "v1" || parts[2] !== "series" || !idPattern.test(parts[3] || "")) {
    return [404, { error: "not_found" }];
  }
  const seriesId = parts[3];
  if (parts.length === 4) {
    const fetched = await loadSeriesDetail(sdk, seriesId);
    if (!fetched) return [404, { error: "not_found" }];
    const { raw, page } = fetched;
    const ids = new Set();
    const episodes = (page.items || []).slice(0, 200)
      .filter(e => idPattern.test(e.id || "") && !ids.has(e.id) && ids.add(e.id))
      .map((e, i) => ({
        id: e.id, number: Number.isSafeInteger(e.number) && e.number > 0 ? e.number : i + 1,
        playable: e.playable === true,
        title: safeText(e.title || "第 " + (i + 1) + " 集", 100),
      })).filter(e => e.number <= 200).sort((a, b) => a.number - b.number);
    if (!episodes.length) return [404, { error: "no_available_episodes" }];
    const info = series({ ...raw, id: seriesId, episode_count: episodes.length }, covers, coverOrigin);
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
export function createCommunityGateway({
  sdk, token, fetchCover = fetchPublicCover, convertHeic = convertHeicToJpeg,
}) {
  if (!sdk || !/^[a-f0-9]{48,128}$/i.test(token || "")) throw Error("Strong local token required");
  const covers = createCoverStore();
  const imageCache = new Map();
  const imageInflight = new Map();
  const waiters = [];
  let activeLoads = 0;
  let cacheBytes = 0;

  const cacheGet = id => {
    const entry = imageCache.get(id);
    if (!entry) return null;
    if (entry.expiresAt <= Date.now()) {
      imageCache.delete(id);
      cacheBytes -= entry.body.length;
      return null;
    }
    imageCache.delete(id);
    imageCache.set(id, entry);
    return entry;
  };
  const cachePut = (id, image) => {
    const previous = imageCache.get(id);
    if (previous) {
      cacheBytes -= previous.body.length;
      imageCache.delete(id);
    }
    if (image.body.length > COVER_CACHE_MAX_BYTES) return;
    imageCache.set(id, { ...image, expiresAt: Date.now() + COVER_CACHE_TTL_MS });
    cacheBytes += image.body.length;
    while (imageCache.size > COVER_CACHE_MAX_ITEMS || cacheBytes > COVER_CACHE_MAX_BYTES) {
      const oldestId = imageCache.keys().next().value;
      const oldest = imageCache.get(oldestId);
      imageCache.delete(oldestId);
      cacheBytes -= oldest.body.length;
    }
  };
  const withLoadSlot = async operation => {
    if (activeLoads >= 4) await new Promise(resolve => waiters.push(resolve));
    activeLoads++;
    try { return await operation(); }
    finally {
      activeLoads--;
      const next = waiters.shift();
      if (next) next();
    }
  };
  const loadImage = id => {
    let pending = imageInflight.get(id);
    if (!pending) {
      pending = withLoadSlot(async () => {
        const remoteUrl = covers.resolve(id);
        if (!remoteUrl) throw new Error("cover_expired");
        const fetched = await fetchCover(remoteUrl);
        const contentType = String(fetched?.contentType || "")
          .split(";")[0].trim().toLowerCase();
        if (!COVER_IMAGE_TYPES.has(contentType)) throw new Error("cover_mime_rejected");
        let body = Buffer.isBuffer(fetched.body) ? fetched.body : Buffer.from(fetched.body || []);
        let outputType = contentType;
        if (HEIC_IMAGE_TYPES.has(contentType)) {
          body = Buffer.from(await convertHeic(body));
          outputType = "image/jpeg";
        }
        if (!body.length || body.length > MAX_COVER_OUTPUT_BYTES) {
          throw new Error("cover_output_rejected");
        }
        return { body, contentType: outputType };
      }).finally(() => imageInflight.delete(id));
      imageInflight.set(id, pending);
    }
    return pending;
  };

  return http.createServer(async (req, res) => {
    const remoteAddress = req.socket.remoteAddress;
    const loopback = remoteAddress === "127.0.0.1" || remoteAddress === "::ffff:127.0.0.1";
    const expectedHost = "127.0.0.1:" + req.socket.localPort;
    const coverMatch = /^\/v1\/covers\/([a-f0-9]{32})$/.exec(req.url || "");

    // Image elements cannot attach the private bridge header. A per-launch,
    // 128-bit opaque ID is the read-only bearer capability for this exact cover.
    if (coverMatch) {
      if (!loopback || req.headers.host !== expectedHost || req.method !== "GET" ||
          (req.headers.origin && req.headers.origin !== "http://tauri.localhost")) {
        replyCoverError(res, 403, "cover_request_forbidden");
        return;
      }
      const id = coverMatch[1];
      if (!covers.resolve(id)) {
        replyCoverError(res, 404, "cover_not_found");
        return;
      }
      try {
        let image = cacheGet(id);
        if (!image) {
          image = await loadImage(id);
          cachePut(id, image);
        }
        replyCoverImage(res, image);
      } catch {
        replyCoverError(res, 502, "cover_unavailable");
      }
      return;
    }

    if (!loopback || req.headers.host !== expectedHost ||
        req.method !== "GET" || req.headers.origin ||
        req.headers["x-hg2a-token"] !== token) {
      reply(res, 403, { error: "local_session_required" }); return;
    }
    let url;
    try { url = new URL(req.url, "http://127.0.0.1"); }
    catch { reply(res, 400, { error: "invalid_route" }); return; }
    if (req.url.length > 1024) { reply(res, 400, { error: "invalid_route" }); return; }
    try {
      const coverOrigin = "http://127.0.0.1:" + req.socket.localPort;
      const [status, body] = await route(sdk, url, { covers, coverOrigin });
      reply(res, status, body);
    } catch (error) {
      const { code, status } = classifySdkError(error);
      // Public SDK error categories only; never print series IDs, CDN URLs,
      // SDK exception messages, request headers, signed tokens or cookies.
      if (url.pathname.startsWith("/v1/series/") &&
          url.pathname.split("/").length === 4) {
        process.stdout.write("HG2B_DETAIL_DIAG code=" + code + " status=" + status + "\n");
      }
      reply(res, status, { error: code });
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
