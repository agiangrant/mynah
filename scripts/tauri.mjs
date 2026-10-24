#!/usr/bin/env node
// Wrapper around the Tauri CLI.
//  - macOS: points cmake-based crates (whisper.cpp) at the same SDK rustc uses, which
//    avoids `ld: tapi error … unknown architecture` when the Command Line Tools SDK is
//    newer than the Xcode toolchain's linker.
//  - macOS: after a successful `build`, re-signs the bundle with the stable local
//    identity from scripts/make-signing-cert.sh when it exists, so privacy grants
//    survive rebuilds. Without it the app keeps Tauri's ad-hoc signature.
import { execSync, spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

if (process.platform === "darwin" && !process.env.SDKROOT) {
  try {
    const sdk = execSync("xcrun --sdk macosx --show-sdk-path", { encoding: "utf8" }).trim();
    if (sdk) process.env.SDKROOT = sdk;
  } catch {
    // xcrun missing: fall through and let the build report its own error
  }
}

const args = process.argv.slice(2);
const isWin = process.platform === "win32";
const result = spawnSync(isWin ? "tauri.cmd" : "tauri", args, {
  stdio: "inherit",
  shell: isWin,
  env: process.env,
});
const status = result.status ?? 1;

if (status === 0 && process.platform === "darwin" && args[0] === "build") {
  for (const app of builtApps()) {
    const sign = spawnSync("bash", [join(root, "scripts", "sign-macos.sh"), app], { stdio: "inherit" });
    if (sign.status !== 0) process.exit(sign.status ?? 1);
  }
}
process.exit(status);

/** Mynah.app bundles produced by this build (default and --target layouts). */
function builtApps() {
  const target = join(root, "src-tauri", "target");
  const candidates = [join(target, "release", "bundle", "macos", "Mynah.app")];
  if (existsSync(target)) {
    for (const triple of readdirSync(target)) {
      candidates.push(join(target, triple, "release", "bundle", "macos", "Mynah.app"));
    }
  }
  return candidates.filter((p) => existsSync(p));
}
