/** Shared front-end defence for optional community cover URLs.
 * New sidecar responses use one fixed opaque loopback route; legacy HTTPS
 * cover URLs retain the existing public-host syntax checks.
 */
export function safeCoverUrl(raw: unknown): string | null {
  if (typeof raw === "string" && /^http:\/\/127\.0\.0\.1:19091\/v1\/covers\/[a-f0-9]{32}$/.test(raw)) {
    return raw;
  }
  if (typeof raw !== "string" || !raw || raw.length > 2048 ||
      /[\x00-\x20\x7f]/.test(raw) || raw.includes("#") || raw.includes("\\")) return null;
  let url: URL;
  try { url = new URL(raw); }
  catch { return null; }
  const host = url.hostname.toLowerCase();
  const suffix = host.split(".").at(-1) ?? "";
  if (url.protocol !== "https:" || url.username || url.password || url.port ||
      !/^[a-z0-9.-]+$/.test(host) || host.includes("..") ||
      host.startsWith(".") || host.endsWith(".") ||
      !/^[a-z]{2,}$/.test(suffix) ||
      ["local", "localhost", "internal", "test", "invalid"].includes(suffix) ||
      /^\d+(?:\.\d+){3}$/.test(host)) return null;
  return raw;
}
