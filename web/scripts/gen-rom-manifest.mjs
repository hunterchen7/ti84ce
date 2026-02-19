#!/usr/bin/env node
import { createHash } from "crypto";
import { readFileSync, writeFileSync } from "fs";
import { gunzipSync } from "zlib";

const rom = readFileSync("public/sys84.bin");
const decompressed = gunzipSync(rom);
const hash = createHash("sha256")
  .update(decompressed)
  .digest("hex")
  .substring(0, 16);

// Auto-increment version from existing manifest if no explicit version given
let prevVersion = 0;
try {
  const prev = JSON.parse(readFileSync("public/rom-manifest.json", "utf-8"));
  prevVersion = prev.version || 0;
} catch {
  /* first run */
}

const version = parseInt(process.argv[2] || String(prevVersion + 1));
const stateVersion = parseInt(process.argv[3] || "10");

const manifest = {
  version,
  romHash: hash,
  stateVersion,
  buildTimestamp: new Date().toISOString(),
};

writeFileSync(
  "public/rom-manifest.json",
  JSON.stringify(manifest, null, 2) + "\n",
);
console.log("Generated rom-manifest.json:", manifest);
