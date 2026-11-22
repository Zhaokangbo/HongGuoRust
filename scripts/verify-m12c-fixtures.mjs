#!/usr/bin/env node
/** Read-only, offline verification; fixed filenames only. */
import { readFile, lstat } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { CASES, validateFixtureManifest, verifyFixtureBytes } from "./m12c-fixture-policy.mjs";

async function main(args) {
  if (args.length !== 2 || args[0] !== "--dir" || !args[1]) {
    throw new Error("Usage: node scripts/verify-m12c-fixtures.mjs --dir <new-fixture-directory>");
  }
  const folder = resolve(args[1]);
  const rootStat = await lstat(folder);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) {
    throw new Error("Fixture directory must not be a symbolic link");
  }
  async function boundedFile(name, limit) {
    // name is an internally fixed basename, never taken from manifest.
    const target = join(folder, name);
    const stat = await lstat(target);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > limit) {
      throw new Error("Unsafe or oversized synthetic fixture: " + name);
    }
    return readFile(target);
  }
  const manifestRaw = await boundedFile("scenario-manifest.json", 16 * 1024);
  const manifest = JSON.parse(manifestRaw.toString("utf8").replace(/^\uFEFF/, ""));
  validateFixtureManifest(manifest);
  const blobs = {};
  for (const row of CASES) {
    blobs[row.fileName] = await boundedFile(row.fileName, 32 * 1024 * 1024);
  }
  const result = verifyFixtureBytes(blobs);
  process.stdout.write(
    "M12-C synthetic fault fixture verification: PASS, " +
    result.verifiedFixtures + " cases\n"
  );
  for (const row of CASES) {
    const bytes = blobs[row.fileName];
    const digest = createHash("sha256").update(bytes).digest("hex");
    process.stdout.write(
      row.id + " " + row.fileName + " size=" + bytes.length + " sha256=" + digest + "\n"
    );
  }
  process.stdout.write("No media data, application settings or network state was modified.\n");
}

main(process.argv.slice(2)).catch(error => {
  process.stderr.write("M12-C fixture verification: " + error.message + "\n");
  process.exitCode = 1;
});
