#!/usr/bin/env node
/**
 * Drift check (#82): re-run openapi-typescript into a temp file, diff it
 * against the committed generated/api.d.ts, and exit non-zero if they differ.
 *
 * Usage (from sdk/typescript/):
 *   node scripts/check-codegen.mjs
 *
 * Run automatically in CI via `npm run codegen:check`.
 */
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, unlinkSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { randomBytes } from "node:crypto";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");
const committed = join(root, "generated", "api.d.ts");
const tmp = join(tmpdir(), `lumenqraph-codegen-${randomBytes(6).toString("hex")}.d.ts`);

// Generate fresh into a temp file.
try {
  execSync(
    `npx openapi-typescript ../../openapi.yaml -o ${tmp}`,
    { cwd: root, stdio: "pipe" },
  );
} catch (err) {
  console.error("codegen failed:", err.message ?? err);
  process.exit(1);
}

// Compare.
const fresh = readFileSync(tmp, "utf8");
const current = existsSync(committed) ? readFileSync(committed, "utf8") : "";
unlinkSync(tmp);

if (fresh !== current) {
  console.error(
    "❌ Generated types are stale!\n" +
    "   Run `npm run codegen` in sdk/typescript/ and commit the result.\n" +
    "\n" +
    "   The OpenAPI schema (openapi.yaml at the repo root) was updated but the\n" +
    "   committed generated/api.d.ts was not regenerated.\n",
  );
  process.exit(1);
}

console.log("✅ Generated types are up to date.");
