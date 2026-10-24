#!/usr/bin/env bash
# Creates a self-signed code-signing certificate in your login keychain so local builds
# keep a stable identity. macOS privacy grants (Accessibility, Input Monitoring, Microphone)
# are tied to the app's code signature; ad-hoc signed builds change identity on every
# rebuild and silently lose those grants. Run once, then build with `pnpm tauri build`
# (scripts/tauri.mjs picks the identity up automatically).
set -euo pipefail
NAME="${1:-Mynah Local Signing}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "\"$NAME\""; then
  echo "identity '$NAME' already exists"; exit 0
fi

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
CNF
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.cnf" >/dev/null 2>&1
# legacy PBE so `security import` accepts OpenSSL 3 output
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -out "$TMP/cert.p12" \
  -passout pass:mynah -name "$NAME" -macalg sha1 -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES
security import "$TMP/cert.p12" -k "$KEYCHAIN" -P mynah -T /usr/bin/codesign -T /usr/bin/security >/dev/null
echo "→ macOS may ask for your login password so codesign can use the key without prompting on every build"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "" "$KEYCHAIN" >/dev/null 2>&1 \
  || security set-key-partition-list -S apple-tool:,apple:,codesign: -s "$KEYCHAIN" >/dev/null 2>&1 \
  || echo "   (skipped: on the first build macOS will show a 'codesign wants to use key' dialog; choose Always Allow)"
echo "→ macOS will ask you to trust the certificate for code signing"
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem"
if security find-identity -v -p codesigning | grep -q "\"$NAME\""; then
  echo "done. Rebuild with: pnpm tauri build   (then grant Accessibility + Input Monitoring one last time)"
else
  echo "identity not usable yet: open Keychain Access → '$NAME' → Trust → Code Signing: Always Trust, then re-run"; exit 1
fi
