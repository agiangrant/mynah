#!/usr/bin/env bash
# `mynah` command installed by the Homebrew formula, which fills in the app bundle
# and libexec paths below at install time.
#
#   mynah                 open Mynah (re-signing it first if your local identity exists)
#   mynah setup-signing   create the local signing identity once, then sign the app
#   mynah sign            sign the app with the existing identity
#   mynah app-path        print the bundle path (for `ln -s` into /Applications)
set -euo pipefail

APP="@APP@"
LIBEXEC="@LIBEXEC@"
IDENTITY="Mynah Local Signing"

sign() {
  bash "$LIBEXEC/sign-macos.sh" "$APP" "$LIBEXEC/entitlements.plist" "$IDENTITY"
}

case "${1:-}" in
  setup-signing)
    bash "$LIBEXEC/make-signing-cert.sh" "$IDENTITY"
    sign
    echo "Open Mynah and grant Accessibility and Input Monitoring once more; they now stick across upgrades."
    ;;
  sign)
    sign
    ;;
  app-path)
    echo "$APP"
    ;;
  -h|--help|help)
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
    ;;
  *)
    # After `brew upgrade` the bundle is a fresh ad-hoc build; restore the stable
    # identity before launching so macOS keeps the permissions you already granted.
    if security find-identity -v -p codesigning 2>/dev/null | grep -q "\"$IDENTITY\""; then
      sign >/dev/null || echo "mynah: re-signing failed, launching anyway" >&2
    fi
    exec open -a "$APP" --args "$@"
    ;;
esac
