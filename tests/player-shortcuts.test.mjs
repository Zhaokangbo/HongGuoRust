import { test } from "node:test";
import assert from "node:assert/strict";
import { resolvePlayerShortcut } from "../src/player/keyboardShortcuts.ts";

const context = {
  catalogOpen: false, fullscreen: false, helpOpen: false,
  connected: true, controllable: true,
  positionSec: 30, durationSec: 100,
  volume: 75, lastAudibleVolume: 75,
};
const shortcut = (key, overrides = {}, state = {}) =>
  resolvePlayerShortcut({ key, ...overrides }, { ...context, ...state });

test("space and K pause only for an active controllable player", () => {
  assert.deepEqual(shortcut(" "), { type: "pause" });
  assert.deepEqual(shortcut("k"), { type: "pause" });
  assert.equal(shortcut(" ", {}, { controllable: false }), null);
  assert.equal(shortcut(" ", {}, { connected: false }), null);
  assert.equal(shortcut(" ", {}, { catalogOpen: true }), null);
});

test("keyboard focus and modifiers never hijack forms or browser shortcuts", () => {
  for (const targetTag of ["INPUT", "TEXTAREA", "SELECT", "BUTTON", "A", "SUMMARY"]) {
    assert.equal(shortcut(" ", { targetTag }), null);
    assert.equal(shortcut("ArrowRight", { targetTag }), null);
  }
  assert.equal(shortcut("m", { contentEditable: true }), null);
  assert.equal(shortcut("j", { targetRole: "textbox" }), null);
  assert.equal(shortcut("j", { targetRole: "searchbox" }), null);
  assert.equal(shortcut(" ", { targetRole: "button" }), null);
  assert.equal(shortcut("ArrowLeft", { targetRole: "slider" }), null);
  assert.equal(shortcut("j", { ctrlKey: true }), null);
  assert.equal(shortcut("j", { altKey: true }), null);
  assert.equal(shortcut("j", { metaKey: true }), null);
  assert.equal(shortcut("j", { isComposing: true }), null);
  assert.equal(shortcut("j", { defaultPrevented: true }), null);
  assert.equal(shortcut("j", { repeat: true }), null);
});

test("seek mapping uses current native snapshot and never seeks outside duration", () => {
  assert.deepEqual(shortcut("ArrowLeft"), { type: "seek", seconds: 25 });
  assert.deepEqual(shortcut("ArrowRight"), { type: "seek", seconds: 35 });
  assert.deepEqual(shortcut("j"), { type: "seek", seconds: 20 });
  assert.deepEqual(shortcut("l"), { type: "seek", seconds: 40 });
  assert.deepEqual(shortcut("j", {}, { positionSec: 2 }), { type: "seek", seconds: 0 });
  assert.deepEqual(shortcut("ArrowRight", {}, { positionSec: 99 }), { type: "seek", seconds: 99.75 });
  assert.equal(shortcut("ArrowRight", {}, { durationSec: null }), null);
  assert.equal(shortcut("ArrowRight", {}, { durationSec: NaN }), null);
  assert.equal(shortcut("ArrowRight", {}, { durationSec: 0 }), null);
});

test("mute remembers last audible level, volume increments in select-compatible steps", () => {
  assert.deepEqual(shortcut("m"), { type: "volume", value: 0 });
  assert.deepEqual(shortcut("m", {}, { volume: 0, lastAudibleVolume: 75 }), { type: "volume", value: 75 });
  assert.deepEqual(shortcut("ArrowUp"), { type: "volume", value: 100 });
  assert.deepEqual(shortcut("ArrowDown"), { type: "volume", value: 50 });
  assert.equal(shortcut("ArrowUp", {}, { volume: 100 }), null);
  assert.equal(shortcut("ArrowDown", {}, { volume: 0 }), null);
});

test("fullscreen keeps F11 and Escape; help takes first Escape", () => {
  assert.deepEqual(shortcut("F11"), { type: "fullscreen", value: true });
  assert.deepEqual(shortcut("F11", { targetTag: "INPUT" }), { type: "fullscreen", value: true });
  assert.deepEqual(shortcut("f"), { type: "fullscreen", value: true });
  assert.deepEqual(shortcut("Escape", {}, { fullscreen: true }), { type: "fullscreen", value: false });
  assert.deepEqual(shortcut("Escape", {}, { fullscreen: true, helpOpen: true }), { type: "help", value: false });
  assert.equal(shortcut("Escape"), null);
  assert.deepEqual(shortcut("?", { shiftKey: true }), { type: "help", value: true });
  assert.deepEqual(shortcut("/", { shiftKey: true }), { type: "help", value: true });
  assert.deepEqual(shortcut("?", {}, { helpOpen: true }), { type: "help", value: false });
  assert.equal(shortcut("?", {}, { catalogOpen: true }), null);
  assert.deepEqual(shortcut("F11", {}, { catalogOpen: true }), { type: "fullscreen", value: true });
});

test("help overlay disables transport shortcuts and F remains guarded", () => {
  assert.equal(shortcut("ArrowRight", {}, { helpOpen: true }), null);
  assert.equal(shortcut("m", {}, { helpOpen: true }), null);
  assert.deepEqual(shortcut("f", {}, { controllable: false }), { type: "fullscreen", value: true });
  assert.equal(shortcut("f", {}, { connected: false }), null);
});
