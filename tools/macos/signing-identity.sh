#!/bin/bash
# Create Snappy's local code-signing identity, once. It lives in its own keychain, whose
# password is kept in a file only the owner can read, so builds sign without prompting.
# Builds signed with it keep the same identity, so macOS keeps the Accessibility grant.
set -euo pipefail

source "$(dirname "$0")/signing-env.sh"

if [[ -f "$SNAPPY_KEYCHAIN" ]]; then
    echo "$SNAPPY_IDENTITY_NAME already exists in $SNAPPY_KEYCHAIN."
    exit 0
fi

mkdir -p "$(dirname "$SNAPPY_KEYCHAIN_PASSWORD_FILE")"
(umask 077 && uuidgen >"$SNAPPY_KEYCHAIN_PASSWORD_FILE")
password="$(cat "$SNAPPY_KEYCHAIN_PASSWORD_FILE")"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat >"$work/cert.conf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $SNAPPY_IDENTITY_NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

# The system LibreSSL writes a PKCS#12 file the keychain can import.
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -config "$work/cert.conf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" \
    -name "$SNAPPY_IDENTITY_NAME" -passout "pass:$password" -out "$work/identity.p12"

security create-keychain -p "$password" "$SNAPPY_KEYCHAIN"
# No automatic locking, so long builds never find it locked.
security set-keychain-settings "$SNAPPY_KEYCHAIN"
security unlock-keychain -p "$password" "$SNAPPY_KEYCHAIN"
security import "$work/identity.p12" -k "$SNAPPY_KEYCHAIN" -P "$password" -T /usr/bin/codesign
# Let codesign use the key without asking.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$SNAPPY_KEYCHAIN" >/dev/null
snappy_search_keychain

echo "Created $SNAPPY_IDENTITY_NAME in $SNAPPY_KEYCHAIN."
