import { test } from "node:test";
import assert from "node:assert/strict";
import {
  selectedTrackId, trackLabel, usableTrackList,
} from "../src/player/mediaTracks.ts";

const audio = [
  { id: 1, selected: true, language: "zh", title: "普通话" },
  { id: 3, selected: false, language: "en", title: "English" },
];
const subtitles = [
  { id: 2, selected: false, language: "zh", title: "中文字幕" },
];
const response = { sessionId: 10, audio, subtitles };

test("track metadata is bound to the exact native session", () => {
  assert.equal(usableTrackList(response, 10), true);
  assert.equal(usableTrackList(response, 11), false);
  assert.equal(usableTrackList(response, null), false);
  assert.equal(usableTrackList(null, 10), false);
  assert.equal(usableTrackList(undefined, 10), false);
  assert.equal(usableTrackList({ ...response, sessionId: 0.5 }, 0.5), false);
  assert.equal(usableTrackList({ ...response, audio: new Array(65).fill(audio[0]) }, 10), false);
  assert.equal(usableTrackList({ ...response, audio: [{ id: -1, selected: true }] }, 10), false);
  assert.equal(usableTrackList({ ...response, audio: [{ id: 1, selected: "yes" }] }, 10), false);
});

test("audio and subtitles identify selection independently", () => {
  assert.equal(selectedTrackId(audio, false), "1");
  assert.equal(selectedTrackId(subtitles, true), "off");
  assert.equal(selectedTrackId([], true), "off");
  assert.equal(selectedTrackId([], false), "");
  assert.equal(selectedTrackId([{ id: 7, selected: true }], true), "7");
});

test("labels show stable track number plus optional sanitized metadata", () => {
  assert.equal(trackLabel(audio[0], 0), "#1 · zh · 普通话");
  assert.equal(trackLabel({ id: 5, selected: false }, 4), "#5");
  assert.equal(trackLabel({ id: 6, selected: false, language: "en", title: "x".repeat(200) }, 1),
    "#2 · en · " + "x".repeat(72));
});
