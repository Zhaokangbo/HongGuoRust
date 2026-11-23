/** M12-C: strict, offline validation of synthetic MP4 fault fixtures.
 * Never traverses directories, follows user-supplied manifest paths or reads
 * arbitrary media outside the explicitly supplied fixture directory.
 */
export const CASES = Object.freeze([
  { id: "healthy", fileName: "healthy.mp4" },
  { id: "empty", fileName: "empty.mp4" },
  { id: "random", fileName: "random-bytes.mp4" },
  { id: "html", fileName: "html-disguised.mp4" },
  { id: "header", fileName: "truncated-header.mp4" },
  { id: "tail", fileName: "truncated-tail.mp4" },
  { id: "extension", fileName: "wrong-extension.txt" },
]);

export function validateFixtureManifest(manifest) {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)
      || manifest.schemaVersion !== 1 || manifest.content !== "locally-generated-synthetic"
      || !Array.isArray(manifest.scenarios) || manifest.scenarios.length !== CASES.length) {
    throw new Error("Unexpected M12-C fixture manifest");
  }
  const expected = new Map(CASES.map(c => [c.id, c.fileName]));
  const ids = new Set();
  for (const scenario of manifest.scenarios) {
    if (!scenario || typeof scenario !== "object" ||
        typeof scenario.id !== "string" ||
        !expected.has(scenario.id) || ids.has(scenario.id) ||
        scenario.fileName !== expected.get(scenario.id) ||
        typeof scenario.expectation !== "string" ||
        !/^[a-zA-Z0-9,; .'\/-]{1,120}$/.test(scenario.expectation)) {
      throw new Error("Unexpected or duplicate M12-C fixture entry");
    }
    ids.add(scenario.id);
  }
  return CASES;
}

function isPrefix(short, long) {
  return short.length <= long.length && long.subarray(0, short.length).equals(short);
}

export function verifyFixtureBytes(blobs) {
  const get = key => {
    const value = blobs[key];
    if (!Buffer.isBuffer(value)) throw new Error("Missing M12-C fixture: " + key);
    return value;
  };
  const good = get("healthy.mp4");
  const empty = get("empty.mp4");
  const random = get("random-bytes.mp4");
  const html = get("html-disguised.mp4");
  const head = get("truncated-header.mp4");
  const tail = get("truncated-tail.mp4");
  const extension = get("wrong-extension.txt");
  if (good.length < 8192 || good.length > 32 * 1024 * 1024 ||
      good.toString("ascii", 4, 8) !== "ftyp") {
    throw new Error("Valid fixture must have a bounded MP4 file type box");
  }
  if (empty.length !== 0) throw new Error("Empty fixture unexpectedly contains data");
  if (random.length !== 256 ||
      !random.every((byte, index) => byte === (index * 73 + 19) % 256)) {
    throw new Error("Random bytes fixture is not deterministic");
  }
  if (html.toString("utf8") !== "<!doctype html><html><body>not an MP4</body></html>") {
    throw new Error("HTML disguise fixture has unexpected content");
  }
  if (head.length < 1 || head.length > 1024 ||
      head.length !== Math.min(1024, Math.floor(good.length / 8)) ||
      !isPrefix(head, good)) {
    throw new Error("Header truncation fixture is invalid");
  }
  if (tail.length !== Math.floor(good.length * 0.75) ||
      !isPrefix(tail, good)) {
    throw new Error("Tail truncation fixture is invalid");
  }
  if (extension.toString("utf8") !== "This is a harmless synthetic text file, not MP4.") {
    throw new Error("Wrong extension test marker was modified");
  }
  return { schemaVersion: 1, verifiedFixtures: CASES.length, goodBytes: good.length };
}
