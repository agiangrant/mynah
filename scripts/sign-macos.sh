#!/usr/bin/env bash
# Sign a built Mynah.app with the stable local identity created by make-signing-cert.sh,
# so macOS privacy grants (Accessibility, Input Monitoring, Microphone) survive rebuilds
# and upgrades. A no-op when the identity does not exist: the app then keeps the ad-hoc
# signature Tauri gave it, which works but is forgotten by macOS on the next build.
#
#   scripts/sign-macos.sh /path/to/Mynah.app [entitlements.plist] [identity name]
set -euo pipefail

APP="${1:?usage: sign-macos.sh /path/to/Mynah.app [entitlements.plist] [identity]}"
ENT="${2:-}"
NAME="${3:-Mynah Local Signing}"
HERE="$(cd "$(dirname "$0")" && pwd)"

if [[ -z "$ENT" ]]; then
  for candidate in "$HERE/../src-tauri/entitlements.plist" "$HERE/entitlements.plist"; do
    if [[ -f "$candidate" ]]; then ENT="$candidate"; break; fi
  done
fi

if ! security find-identity -v -p codesigning 2>/dev/null | grep -q "\"$NAME\""; then
  echo "sign-macos: no '$NAME' identity, keeping the ad-hoc signature (run make-signing-cert.sh once to keep permissions across rebuilds)"
  exit 0
fi
if codesign -dv --verbose=2 "$APP" 2>&1 | grep -q "Authority=$NAME"; then
  echo "sign-macos: $APP is already signed by '$NAME'"
  exit 0
fi

args=(--force --deep --sign "$NAME")
if [[ -n "$ENT" && -f "$ENT" ]]; then
  args+=(--entitlements "$ENT")
fi
codesign "${args[@]}" "$APP"
codesign --verify --deep --strict "$APP"
echo "sign-macos: signed $APP with '$NAME'"
