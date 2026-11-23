import { test } from "node:test";
import assert from "node:assert/strict";
import { previewTimeAtPointer, previewBucket, validPreviewFrame } from "../src/player/thumbnailPreview.ts";

test("mouse pointer maps to timeline with strict bounds", () => {
  assert.equal(previewTimeAtPointer(100, 100, 200, 20), 0);
  assert.equal(previewTimeAtPointer(200, 100, 200, 20), 10);
  assert.equal(previewTimeAtPointer(300, 100, 200, 20), 20);
  assert.equal(previewTimeAtPointer(-50, 100, 200, 20), 0);
  assert.equal(previewTimeAtPointer(900, 100, 200, 20), 20);
  assert.equal(previewTimeAtPointer(150, 100, 0, 20), null);
  assert.equal(previewTimeAtPointer(150, 100, 200, 0), null);
  assert.equal(previewTimeAtPointer(NaN, 100, 200, 20), null);
});

test("5-second bucket quantization throttles adjacent hover positions", () => {
  assert.equal(previewBucket(0), 0);
  assert.equal(previewBucket(4.99), 0);
  assert.equal(previewBucket(5), 5);
  assert.equal(previewBucket(9.5), 5);
  assert.equal(previewBucket(27), 25);
  assert.equal(previewBucket(NaN), null);
  assert.equal(previewBucket(-1), null);
  assert.equal(previewBucket(90000), null);
});

test("only same-session JPEG data from Rust may enter preview DOM", () => {
  const frame = { sessionId: 9, timeSec: 5, dataUrl: "data:image/jpeg;base64,/9j/2Q==" };
  assert.equal(validPreviewFrame(frame, 9), true);
  assert.equal(validPreviewFrame(frame, 10), false);
  assert.equal(validPreviewFrame(frame, null), false);
  assert.equal(validPreviewFrame(null, 9), false);
  assert.equal(validPreviewFrame({ ...frame, dataUrl: "https://example.com/secret.mp4" }, 9), false);
  assert.equal(validPreviewFrame({ ...frame, timeSec: NaN }, 9), false);
  assert.equal(validPreviewFrame({ ...frame, dataUrl: "data:image/png;base64,xxx" }, 9), false);
  assert.equal(validPreviewFrame({ ...frame, dataUrl: "data:image/jpeg;base64," + "A".repeat(600000) }, 9), false);
});
