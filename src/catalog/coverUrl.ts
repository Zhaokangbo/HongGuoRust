/** Shared front-end defence for the optional community SDK's remote covers.
 * A cover is visual metadata, never a media playback target.
 * Safe syntax is not proof of DNS/redirect safety or content rights.
 */
export function safeCoverUrl(raw: unknown): string | null {
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
