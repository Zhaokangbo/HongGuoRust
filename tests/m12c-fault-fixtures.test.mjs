import { test } from "node:test";
import assert from "node:assert/strict";
import { CASES, validateFixtureManifest, verifyFixtureBytes } from "../scripts/m12c-fixture-policy.mjs";

const fixtures = () => {
  const healthy = Buffer.alloc(16384);
  healthy.write("ftyp", 4, "ascii");
  const random = Buffer.from(Array.from({ length: 256 }, (_, index) => (index * 73 + 19) % 256));
  return {
    "healthy.mp4": healthy,
    "empty.mp4": Buffer.alloc(0),
    "random-bytes.mp4": random,
    "html-disguised.mp4": Buffer.from("<!doctype html><html><body>not an MP4</body></html>", "utf8"),
    "truncated-header.mp4": Buffer.from(healthy.subarray(0, 1024)),
    "truncated-tail.mp4": Buffer.from(healthy.subarray(0, Math.floor(healthy.length * 0.75))),
    "wrong-extension.txt": Buffer.from("This is a harmless synthetic text file, not MP4.", "utf8"),
  };
};

const manifest = () => ({
  schemaVersion: 1,
  content: "locally-generated-synthetic",
  scenarios: CASES.map(({ id, fileName }) => ({
    id,
    fileName,
    expectation: id === "tail"
      ? "may play partially or fail; never crash/hang"
      : "error or rejection",
  })),
});

test("M12-C expected manifest accepts all seven fixed, non-sensitive basenames", () => {
  assert.equal(validateFixtureManifest(manifest()).length, 7);
  assert.deepEqual(verifyFixtureBytes(fixtures()), {
    schemaVersion: 1, verifiedFixtures: 7, goodBytes: 16384,
  });
});

test("reject malformed manifests, duplicate IDs and file traversal", () => {
  const expected = manifest();
  assert.throws(() => validateFixtureManifest({ ...expected, schemaVersion: 2 }), /manifest/);
  assert.throws(() => validateFixtureManifest({ ...expected, content: "https://secret" }), /manifest/);
  const changed = manifest();
  changed.scenarios[2].fileName = "../../media/private.mp4";
  assert.throws(() => validateFixtureManifest(changed), /fixture entry/);
  const duplicate = manifest();
  duplicate.scenarios[2].id = duplicate.scenarios[1].id;
  assert.throws(() => validateFixtureManifest(duplicate), /duplicate/);
  const hostile = manifest();
  hostile.scenarios[1].expectation = "file:///D:/secret.mp4?token=abc";
  assert.throws(() => validateFixtureManifest(hostile), /fixture entry/);
});

test("reject corrupted or missing fixture variants before owner tests", () => {
  const original = fixtures();
  for (const bad of [
    { ...original, "healthy.mp4": Buffer.alloc(16384) },
    { ...original, "empty.mp4": Buffer.from([0]) },
    { ...original, "random-bytes.mp4": Buffer.alloc(256) },
    { ...original, "truncated-header.mp4": Buffer.from([0]) },
    { ...original, "truncated-tail.mp4": Buffer.from([1, 2]) },
    { ...original, "html-disguised.mp4": Buffer.from("not-html") },
    { ...original, "wrong-extension.txt": Buffer.from("secret") },
    { ...original, "empty.mp4": undefined },
  ]) {
    assert.throws(() => verifyFixtureBytes(bad));
  }
});

test("fixture definitions never include URLs, paths, account data or shell commands", () => {
  const source = JSON.stringify(CASES);
  assert.doesNotMatch(source, /https?:|token|password|\\\\|\.{2}\//i);
  assert.equal(new Set(CASES.map(x => x.fileName)).size, CASES.length);
});
