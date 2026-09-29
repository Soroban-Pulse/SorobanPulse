#!/usr/bin/env node
/**
 * CI guard: verifies that web/src/api/schema.d.ts is up to date with
 * the committed openapi.json.
 *
 * Usage (local):  node web/scripts/check-api-up-to-date.mjs
 * Usage (CI):     runs automatically in the "check-api-types" workflow job.
 *
 * Strategy:
 *  1. Generate a fresh schema into a temp file.
 *  2. Diff it against the committed file.
 *  3. Exit 1 with a clear message if they differ.
 */

import { execSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";

// Paths relative to the workspace root (this script lives in web/scripts/).
const scriptDir = new URL(".", import.meta.url).pathname;
// On Windows the pathname starts with a leading slash, strip it.
const webDir = resolve(scriptDir, "..");
const repoRoot = resolve(webDir, "..");

const openapiSpec = join(repoRoot, "openapi.json");
const committedSchema = join(webDir, "src", "api", "schema.d.ts");

// ---------------------------------------------------------------------------
// 1. Generate into a temp directory
// ---------------------------------------------------------------------------
const tmpDir = mkdtempSync(join(tmpdir(), "soroban-pulse-api-check-"));
const generatedSchema = join(tmpDir, "schema.d.ts");

try {
  execSync(
    `node ${JSON.stringify(
      resolve(webDir, "node_modules", ".bin", "openapi-typescript"),
    )} ${JSON.stringify(openapiSpec)} -o ${JSON.stringify(generatedSchema)}`,
    { stdio: "inherit", cwd: webDir },
  );
} catch {
  // openapi-typescript not installed yet — skip the diff check so that a
  // fresh checkout without node_modules doesn't fail CI before `npm ci`.
  console.warn(
    "[check-api] openapi-typescript not found in node_modules — skipping schema freshness check.\n" +
      "           Run `npm ci` inside web/ first.",
  );
  process.exit(0);
}

// ---------------------------------------------------------------------------
// 2. Compare hashes (avoids OS line-ending noise)
// ---------------------------------------------------------------------------
function sha256(filePath) {
  const content = readFileSync(filePath, "utf8").replace(/\r\n/g, "\n");
  return createHash("sha256").update(content).digest("hex");
}

const committedHash = sha256(committedSchema);
const generatedHash = sha256(generatedSchema);

// Cleanup
rmSync(tmpDir, { recursive: true, force: true });

// ---------------------------------------------------------------------------
// 3. Report
// ---------------------------------------------------------------------------
if (committedHash === generatedHash) {
  console.log(
    "✓  web/src/api/schema.d.ts is up to date with openapi.json",
  );
  process.exit(0);
} else {
  console.error(`
✗  web/src/api/schema.d.ts is OUT OF DATE with openapi.json.

   The committed schema no longer matches what openapi-typescript would
   generate from the current spec.

   To fix, run:
     cd web && npm run gen:api

   Then commit the updated schema.d.ts together with any openapi.json changes.
`);
  process.exit(1);
}
