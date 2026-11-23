import { test } from "node:test";
import assert from "node:assert/strict";
import { publicCoverUrl, isApprovedCoverUrl } from "../scripts/hg2a-sdk/server.mjs";
import { safeCoverUrl } from "../src/catalog/coverUrl.ts";

const accepted = [
  "https://covers.example.org/a.webp",
  "https://img.example.net/covers/novel~tplv-a.webp?x-expires=1892345678&sig=abc%2F123",
  "https://p26-a.example.com/cover.jpeg?image_type=webp",
];
const rejected = [
  "http://127.0.0.1:19092/v1/covers/0123456789abcdef0123456789abcdef",
  "http://127.0.0.1:19091/v1/covers/not-a-token",
  "http://127.0.0.1:19091/v1/covers/0123456789abcdef0123456789abcdef?url=https://evil.example",
  "http://cdn.example.com/a.jpg",
  "//cdn.example.com/a.jpg",
  "file:///C:/Users/test/private.png",
  "data:image/svg+xml;base64,AAA",
  "https://user:secret@cdn.example.com/cover.jpg",
  "https://cdn.example.com:8443/cover.webp",
  "https://127.0.0.1/a.jpg",
  "https://192.168.1.2/a.jpg",
  "https://localhost/a.jpg",
  "https://images.internal/a.jpg",
  "https://images.test/a.webp",
  "https://cdn.example.com/a.jpg#fragment",
  "https://foo..example.com/image.png",
  "https://cdn.example.com/cover.webp\nHeader: injected",
  "https://cdn.example.com/cover.webp\\nHeader: escaped",
  "https://example.com/" + "x".repeat(2100),
];

test("HG-2B image relay only approves the observed community poster CDN", () => {
  assert.equal(isApprovedCoverUrl("https://p3-reading-sign.fqnovelpic.com/poster.heic?sig=test"), true);
  assert.equal(isApprovedCoverUrl("https://fqnovelpic.com/poster.heic"), true);
  assert.equal(isApprovedCoverUrl("https://fqnovelpic.com.evil.example/poster.heic"), false);
  assert.equal(isApprovedCoverUrl("https://covers.example.org/poster.jpg"), false);
});

test("HG-2B UI accepts only the fixed opaque loopback cover URL", () => {
  const local = "http://127.0.0.1:19091/v1/covers/0123456789abcdef0123456789abcdef";
  assert.equal(safeCoverUrl(local), local);
  assert.equal(publicCoverUrl(local), null, "the upstream URL validator remains HTTPS-only");
});

test("HG-2B legitimate external HTTPS cover URLs survive without changing signed query bytes", () => {
  for (const value of accepted) {
    assert.equal(publicCoverUrl(value), value);
    assert.equal(safeCoverUrl(value), value);
  }
});

test("HG-2B both Node bridge and UI reject unsafe remote poster URLs", () => {
  for (const value of rejected) {
    assert.equal(publicCoverUrl(value), null, "Node sidecar must reject " + value.slice(0, 64));
    assert.equal(safeCoverUrl(value), null, "Tauri WebView must reject " + value.slice(0, 64));
  }
});

test("HG-2B missing cover is optional and does not prevent valid real metadata", () => {
  for (const value of [null, undefined, "", 12, {}, []]) {
    assert.equal(publicCoverUrl(value), null);
    assert.equal(safeCoverUrl(value), null);
  }
});
