import { test } from "node:test";
import assert from "node:assert/strict";
import { volumeAfterMute } from "../src/player/volume.ts";

test("mute switches a non-zero volume to zero", () => {
  assert.equal(volumeAfterMute(68, 68), 0);
  assert.equal(volumeAfterMute(1, 40), 0);
});

test("unmute restores the last audible level within supported bounds", () => {
  assert.equal(volumeAfterMute(0, 68), 68);
  assert.equal(volumeAfterMute(0, 0), 1);
  assert.equal(volumeAfterMute(0, 145), 100);
  assert.equal(volumeAfterMute(0, -20), 1);
});

test("invalid remembered volume falls back to a safe audible level", () => {
  assert.equal(volumeAfterMute(0, Number.NaN), 100);
  assert.equal(volumeAfterMute(Number.NaN, Number.POSITIVE_INFINITY), 100);
});
