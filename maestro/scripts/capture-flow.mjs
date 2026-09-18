#!/usr/bin/env node

import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { homedir } from "node:os";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "../..");
const artifactsRoot = resolve(root, "maestro/artifacts/runs");
const registry = JSON.parse(readFileSync(resolve(root, "maestro/flows/registry.json"), "utf8"));

function usage(message) {
  if (message) console.error(`Error: ${message}\n`);
  console.error("Usage: pnpm flow:capture [--flow <flow-id>] [--platform ios|android|ios,android] [--ios-udid <id>] [--android-udid <id>] [--append-run <run-id>] [--skip-permission-setup]");
  process.exit(1);
}

function readOption(args, option) {
  const index = args.indexOf(option);
  if (index === -1) return undefined;
  if (!args[index + 1] || args[index + 1].startsWith("--")) usage(`${option} requires a value`);
  const value = args[index + 1];
  args.splice(index, 2);
  return value;
}

const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const requestedFlow = readOption(args, "--flow");
const requestedPlatforms = readOption(args, "--platform") ?? "ios,android";
const iosUdid = readOption(args, "--ios-udid") ?? process.env.MAESTRO_IOS_UDID;
const androidUdid = readOption(args, "--android-udid") ?? process.env.MAESTRO_ANDROID_UDID;
const appendRunId = readOption(args, "--append-run");
const skipPermissionSetup = args.includes("--skip-permission-setup");
if (skipPermissionSetup) args.splice(args.indexOf("--skip-permission-setup"), 1);
if (args.length) usage(`unknown option ${args[0]}`);

const flow = requestedFlow
  ? registry.flows.find((candidate) => candidate.id === requestedFlow)
  : registry.flows.length === 1
    ? registry.flows[0]
    : null;
if (!flow) usage(requestedFlow ? `unknown flow ${requestedFlow}` : "choose a flow with --flow");

const platforms = [...new Set(requestedPlatforms.split(",").filter(Boolean))];
if (!platforms.length || platforms.some((platform) => platform !== "ios" && platform !== "android")) {
  usage("--platform accepts ios, android, or ios,android");
}
if (appendRunId && !/^[A-Za-z0-9-]+$/.test(appendRunId)) {
  usage("--append-run must be a capture run ID");
}

const runId = appendRunId ?? new Date().toISOString().replace(/[:.]/g, "-");
const runDir = resolve(artifactsRoot, runId);
const definition = JSON.parse(readFileSync(resolve(root, flow.definition), "utf8"));
const manifestPath = resolve(runDir, "manifest.json");
let manifest;
if (appendRunId) {
  if (!existsSync(manifestPath)) usage(`No captured flow exists at ${runDir}`);
  manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.flows?.[0]?.id !== flow.id) {
    usage(`Run ${runId} belongs to a different flow`);
  }
} else {
  mkdirSync(runDir, { recursive: true });
  manifest = {
    schemaVersion: 1,
    runId,
    createdAt: new Date().toISOString(),
    flows: [{
      ...definition,
      screenshots: definition.screens.map((screen, index) => ({ ...screen, order: index + 1 })),
    }],
    platforms: {},
  };
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, { cwd: root, stdio: "inherit", ...options });
  return result.status === 0;
}

function preparePermissions(platform, udid) {
  if (skipPermissionSetup) return true;
  if (platform === "ios") {
    // iOS Password AutoFill can intercept Maestro's explicit SecureField input.
    // This is the same simulator profile setting used by the iOS demo runner.
    const settingsProfile = resolve(
      homedir(),
      "Library/Developer/CoreSimulator/Devices",
      udid,
      "data/Containers/Shared/SystemGroup/systemgroup.com.apple.configurationprofiles/Library/ConfigurationProfiles/UserSettings.plist",
    );
    const passwordAutofillDisabled = existsSync(settingsProfile)
      && run("/usr/bin/plutil", [
        "-replace",
        "restrictedBool.allowPasswordAutoFill.value",
        "-bool",
        "NO",
        settingsProfile,
      ]);
    return passwordAutofillDisabled
      && run("xcrun", ["simctl", "privacy", udid, "grant", "photos", "com.lasco.lasco"]);
  }

  // Android 13+ uses the first two permissions; ACCESS_MEDIA_LOCATION preserves
  // the original metadata when the image provides it. Older API levels safely
  // report an unsupported permission and are handled by --skip-permission-setup.
  return [
    "android.permission.READ_MEDIA_IMAGES",
    "android.permission.READ_MEDIA_VIDEO",
    "android.permission.ACCESS_MEDIA_LOCATION",
  ].every((permission) => run("adb", ["-s", udid, "shell", "pm", "grant", "com.lasco.lasco", permission]));
}

function flattenScreenshots(platformDir) {
  for (const testRun of readdirSync(platformDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => resolve(platformDir, entry.name))
    .sort()
    .reverse()) {
    for (const flowRun of readdirSync(testRun, { withFileTypes: true }).filter((entry) => entry.isDirectory())) {
      const source = resolve(testRun, flowRun.name, "takeScreenshot/screenshots");
      if (existsSync(source)) {
        cpSync(source, resolve(platformDir, "screenshots"), { recursive: true });
        return;
      }
    }
  }
}

const udids = { ios: iosUdid, android: androidUdid };
const maestroBin = process.env.MAESTRO_BIN ?? "maestro";
for (const platform of platforms) {
  const udid = udids[platform];
  if (!udid) {
    manifest.platforms[platform] = { status: "not-run", error: `Provide --${platform}-udid or MAESTRO_${platform.toUpperCase()}_UDID.` };
    continue;
  }

  const platformDir = resolve(runDir, platform);
  mkdirSync(platformDir, { recursive: true });
  if (!preparePermissions(platform, udid)) {
    manifest.platforms[platform] = { status: "failed", error: "Could not grant media permissions." };
    continue;
  }

  const passed = run(maestroBin, [
    "--udid", udid,
    "test",
    "--test-output-dir", relative(root, platformDir),
    resolve(root, flow.flow),
  ]);
  flattenScreenshots(platformDir);
  manifest.platforms[platform] = {
    status: passed ? "passed" : "failed",
    artifactDirectory: relative(root, platformDir),
  };
}

manifest.status = Object.values(manifest.platforms).every((platform) => platform.status === "passed")
  ? "passed"
  : "failed";
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

console.log(`\nArtifacts: ${relative(root, runDir)}`);
console.log(`Viewer: pnpm flow:view -- --run ${runId}`);
process.exit(manifest.status === "passed" ? 0 : 1);
