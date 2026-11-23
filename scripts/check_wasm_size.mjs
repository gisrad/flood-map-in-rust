#!/usr/bin/env node
// Brotli (quality 11) size budget for the WebAssembly module that Trunk builds.
//
//   node scripts/check_wasm_size.mjs [--dist crates/flood-web/dist] [--budget-kib 450]
//
// Exit codes: 0 within budget, 1 over budget, 2 usage error or missing/ambiguous .wasm.
// When $GITHUB_STEP_SUMMARY is set, a Markdown table is appended to it.
// Budget unit is KiB (1024 bytes); it applies to the compressed size, as a CDN would send it.

import { appendFileSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const KIB = 1024;
const WASM_NAME = /^flood-web-[0-9a-f]+_bg\.wasm$/;

function usageError(message) {
  console.error(`check_wasm_size: ${message}`);
  process.exit(2);
}

let values;
try {
  ({ values } = parseArgs({
    options: {
      dist: { type: "string", default: "crates/flood-web/dist" },
      "budget-kib": { type: "string", default: "450" },
    },
    allowPositionals: false,
  }));
} catch (error) {
  usageError(error.message);
}

const budgetKib = Number(values["budget-kib"]);
if (!Number.isFinite(budgetKib) || budgetKib <= 0) {
  usageError(`--budget-kib must be a positive number, got "${values["budget-kib"]}"`);
}
const budgetBytes = Math.floor(budgetKib * KIB);

const dist = resolve(values.dist);
let entries;
try {
  entries = readdirSync(dist);
} catch {
  usageError(`cannot read ${dist}; run "trunk build --release --locked" in crates/flood-web first`);
}
const modules = entries.filter((name) => WASM_NAME.test(name));
if (modules.length !== 1) {
  usageError(
    modules.length === 0
      ? `no flood-web-<hash>_bg.wasm in ${dist}`
      : `expected exactly one flood-web-<hash>_bg.wasm in ${dist}, found: ${modules.join(", ")}`,
  );
}

const name = modules[0];
const raw = readFileSync(join(dist, name));
if (raw.length < 4 || raw.readUInt32BE(0) !== 0x0061736d) {
  usageError(`${name} does not start with the WebAssembly magic number`);
}

const brotli = brotliCompressSync(raw, {
  params: {
    [constants.BROTLI_PARAM_MODE]: constants.BROTLI_MODE_GENERIC,
    [constants.BROTLI_PARAM_QUALITY]: 11,
    [constants.BROTLI_PARAM_SIZE_HINT]: raw.length,
  },
});
const gzip = gzipSync(raw, { level: 9 });

const kib = (bytes) => `${(bytes / KIB).toFixed(1)} KiB`;
const withBytes = (bytes) => `${kib(bytes)} (${bytes.toLocaleString("en-US")} B)`;
const withinBudget = brotli.length <= budgetBytes;
const headroom = budgetBytes - brotli.length;

console.log(`WASM size budget (Brotli quality 11)`);
console.log(`  file      ${name}`);
console.log(`  raw       ${withBytes(raw.length)}`);
console.log(`  gzip -9   ${withBytes(gzip.length)}`);
console.log(`  brotli 11 ${withBytes(brotli.length)}`);
console.log(`  budget    ${withBytes(budgetBytes)}`);
console.log(
  withinBudget
    ? `  result    PASS, ${kib(headroom)} below the budget`
    : `  result    FAIL, ${kib(-headroom)} over the budget`,
);

const summaryPath = process.env.GITHUB_STEP_SUMMARY;
if (summaryPath) {
  appendFileSync(
    summaryPath,
    [
      "### WASM size budget (Brotli quality 11)",
      "",
      "| File | Raw | gzip -9 | Brotli q11 | Budget | Result |",
      "|---|---:|---:|---:|---:|---|",
      `| \`${name}\` | ${kib(raw.length)} | ${kib(gzip.length)} | ${kib(brotli.length)} | ${kib(budgetBytes)} | ${withinBudget ? "pass" : "**FAIL**"} |`,
      "",
    ].join("\n"),
  );
}

if (!withinBudget) {
  console.error(
    "check_wasm_size: the WASM module is over budget. Reduce dependencies/features, or raise " +
      "WASM_BROTLI_BUDGET_KIB in .github/workflows/ci.yml deliberately and say why in the change.",
  );
  process.exit(1);
}
