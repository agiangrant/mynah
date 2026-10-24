# Mynah

Fast, private, on-device speech to text for macOS, Windows and Linux. Hold a key, speak, release: the words land wherever your cursor is. Nothing leaves your machine.

## Features

- **Hold-to-dictate** global shortcut (any key or chord, including modifier-only keys such as Right ⌘ / fn). Hold, toggle and auto (tap-to-toggle, hold-to-talk) modes. Escape cancels.
- **Local Whisper engine** via whisper.cpp with Metal (macOS), Vulkan (Windows default) or CUDA (`--features cuda`) acceleration. Model catalog from tiny to large-v3-turbo and distil-large-v3, quantized variants, resumable downloads, and import of your own GGML/GGUF models.
- **Filler word removal** (uh, um, hmm, er, …) with an editable list, plus a hallucination filter for silent audio.
- **Custom vocabulary with pronunciations**: words are fed to the model as a prompt, boosted while it decodes (token-trie shallow fusion in whisper.cpp's logits callback, strength configurable, never penalising), and anything that sounds like a word or one of its "sounds like" spellings is rewritten afterwards (for example `flicker` → `Flickr`).
- **Meeting detection**: notices Zoom, Teams, Webex, FaceTime, Slack, Discord and others (editable list) using the running process list and, on macOS and Linux, whether the microphone is actually in use. Prompts or auto-starts a transcript.
- **Built for long recordings**: dictation audio is segmented at pauses and transcribed while you are still talking, so memory stays flat and a 30-minute dictation is inserted seconds after you release the key (hard stop at 3 h). Meeting transcripts are written to disk every 20 s, so a crash or forced quit loses at most that much. All synthetic keystrokes are posted from the main thread, which macOS requires for the paste shortcut.
- **Meeting transcription** with timestamps, split into You / Others when a system audio device is configured, saved as Markdown or text.
- **Floating status pill** that never steals focus, tray icon, launch at login, history with search, insert by typing or paste.

## Install

Every route builds from source on your machine: no download to trust, no signing fee to pay, and because the binary was compiled locally there is no Gatekeeper or SmartScreen quarantine. The toolchains come from the package manager; the first build takes 10–20 minutes.

**macOS (Homebrew)**, needs the Command Line Tools (`xcode-select --install`):

```sh
brew tap agiangrant/mynah https://github.com/agiangrant/mynah
brew install agiangrant/mynah/mynah      # or --HEAD for the main branch
mynah setup-signing                      # once: local signing identity so permissions survive upgrades
mynah                                    # or: ln -sfn "$(mynah app-path)" /Applications/Mynah.app
```

**Windows (Scoop)**, needs Visual Studio Build Tools with the C++ workload (`winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"`):

```powershell
scoop bucket add mynah https://github.com/agiangrant/mynah
scoop install mynah                      # pulls rustup, Node, pnpm, cmake and the Vulkan SDK
```

**Linux (snap)**: `snap install mynah` once the Store listing is live. Until then, or to build it yourself: `snapcraft` in the repo root, then `sudo snap install --dangerous mynah_*.snap`. The global shortcut and text insertion use X11; on Wayland run it under XWayland.

The CI workflow in `.github/workflows/packages.yml` runs all three installs from scratch on every push.

## Stack

Tauri 2 + Rust backend (`src-tauri`), React 19 + TypeScript frontend (`src`). Audio capture with cpal, key listening with rdev, text injection with enigo, inference with whisper-rs.

## Development

Prerequisites: Rust 1.85+, Node 20+ with pnpm, CMake, a C/C++ toolchain (Xcode CLT on macOS, MSVC Build Tools on Windows, `build-essential` + `libasound2-dev libxdo-dev libxi-dev libxtst-dev libwebkit2gtk-4.1-dev` on Linux).

```sh
pnpm install
pnpm tauri dev
```

Release build: `pnpm tauri build`. Windows with CUDA: `pnpm tauri build -- --features cuda`.

### macOS permissions

Mynah needs **Microphone**, **Input Monitoring** (to see the dictation key) and **Accessibility** (to type into other apps). The General page shows the status of each and opens the right System Settings pane. Without Accessibility, macOS silently drops the keystrokes Mynah sends to other apps; Mynah detects that, copies the text to the clipboard and tells you.

Grants are tied to the app's code signature. Ad-hoc signed builds get a new identity on every rebuild, so the toggles in System Settings can show "on" while no longer applying to the current binary (fix: toggle Mynah off and on again in each list). To stop that for good, create a stable local signing identity once:

```sh
scripts/make-signing-cert.sh
pnpm tauri build   # picks up "Mynah Local Signing" automatically
```

Logs: `~/Library/Application Support/app.mynah.desktop/mynah.log` (previous run in `mynah.log.1`).

`pnpm tauri` goes through `scripts/tauri.mjs`, which sets `SDKROOT` to Xcode's SDK on macOS. That avoids `ld: tapi error: malformed file … unknown architecture` when the Command Line Tools SDK is newer than the linker. If you call `cargo` directly, export it yourself:

```sh
export SDKROOT=$(xcrun --sdk macosx --show-sdk-path)
```

### Offline benchmark

```sh
say -o /tmp/s.wav --data-format=LEI16@16000 "Schedule the design review for Thursday at ten."
cd src-tauri && cargo run --release --example bench -- ~/Library/Application\ Support/app.mynah.desktop/models/ggml-large-v3-turbo.bin /tmp/s.wav
```

### Tests

```sh
cd src-tauri && cargo test
```

## Data locations

Settings, history, models and meeting transcripts live in the platform app-data directory (macOS: `~/Library/Application Support/app.mynah.desktop`).

## Known limits

- System-audio capture for meetings needs a loopback source: Windows pick your speakers (WASAPI loopback), macOS a BlackHole/aggregate device, Linux a PulseAudio monitor source.
- Wayland sessions restrict global key listening and synthetic typing for every app of this kind; X11 works fully.
- Microphone-in-use detection is unavailable on Windows, so meeting detection there relies on the process list alone.
