#!/usr/bin/env node
// Wrapper around the Tauri CLI. On macOS it points cmake-based crates (whisper.cpp)
// at the same SDK rustc uses, which avoids `ld: tapi error … unknown architecture`
// when the Command Line Tools SDK is newer than the Xcode toolchain's linker.
import { execSync, spawnSync } from "node:child_process";

if (process.platform === "darwin" && !process.env.SDKROOT) {
  try {
    const sdk = execSync("xcrun --sdk macosx --show-sdk-path", { encoding: "utf8" }).trim();
    if (sdk) process.env.SDKROOT = sdk;
  } catch {
    // xcrun missing: fall through and let the build report its own error
  }
}

// Prefer the stable local signing identity (see scripts/make-signing-cert.sh) so macOS
// privacy grants survive rebuilds. Falls back to ad-hoc when it does not exist.
if (process.platform === "darwin" && !process.env.APPLE_SIGNING_IDENTITY) {
  try {
    const list = execSync("security find-identity -v -p codesigning", { encoding: "utf8" });
    const m = list.match(/"(Mynah Local Signing[^"]*)"/);
    if (m) process.env.APPLE_SIGNING_IDENTITY = m[1];
  } catch {
    // no keychain access: ad-hoc signing
  }
}

const isWin = process.platform === "win32";
const result = spawnSync(isWin ? "tauri.cmd" : "tauri", process.argv.slice(2), {
  stdio: "inherit",
  shell: isWin,
  env: process.env,
});
process.exit(result.status ?? 1);
