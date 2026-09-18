#!/usr/bin/env node

import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "../..");
const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const runIndex = args.indexOf("--run");
const restage = args.includes("--restage");
if (
  runIndex === -1 ||
  !args[runIndex + 1] ||
  args.length !== (restage ? 3 : 2)
) {
  console.error("Usage: pnpm flow:view -- --run <run-id> [--restage]");
  process.exit(1);
}

const runId = args[runIndex + 1];
if (!/^[A-Za-z0-9-]+$/.test(runId)) {
  console.error("Run ID may contain only letters, numbers, and hyphens.");
  process.exit(1);
}

const source = resolve(root, "maestro/artifacts/runs", runId);
const destination = resolve(root, "maestro/viewer/public/runs", runId);
if (!existsSync(resolve(source, "manifest.json"))) {
  console.error(`No captured flow exists at ${source}`);
  process.exit(1);
}
if (existsSync(destination)) {
  if (!restage) {
    console.error(`Viewer assets already staged for ${runId}. Re-run with --restage to refresh this generated directory.`);
    process.exit(1);
  }
  rmSync(destination, { recursive: true, force: true });
}

mkdirSync(resolve(root, "maestro/viewer/public/runs"), { recursive: true });
cpSync(source, destination, { recursive: true });
const result = spawnSync("pnpm", ["--dir", "maestro/viewer", "dev", "--host"], {
  cwd: root,
  stdio: "inherit",
  env: { ...process.env, VITE_FLOW_RUN: runId },
});
process.exit(result.status ?? 1);
