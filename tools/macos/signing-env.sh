# Where Snappy's local signing identity lives. Sourced by the macOS build scripts.
SNAPPY_IDENTITY_NAME="Snappy Local Signing"
SNAPPY_KEYCHAIN="$HOME/Library/Keychains/snappy-signing.keychain-db"
SNAPPY_KEYCHAIN_PASSWORD_FILE="$HOME/.config/snappy/signing-keychain-password"

# Put the keychain on the user's search list, where codesign looks for keys.
snappy_search_keychain() {
    local listed
    listed="$(security list-keychains -d user | tr -d '" ')"
    grep -qxF "$SNAPPY_KEYCHAIN" <<<"$listed" && return 0
    # shellcheck disable=SC2086
    security list-keychains -d user -s $listed "$SNAPPY_KEYCHAIN"
}

# Print the identity's SHA-1 hash, ready for codesign, or nothing when there is none.
snappy_signing_identity() {
    [[ -f "$SNAPPY_KEYCHAIN" && -f "$SNAPPY_KEYCHAIN_PASSWORD_FILE" ]] || return 0
    snappy_search_keychain
    security unlock-keychain -p "$(cat "$SNAPPY_KEYCHAIN_PASSWORD_FILE")" "$SNAPPY_KEYCHAIN"
    security find-certificate -c "$SNAPPY_IDENTITY_NAME" -Z "$SNAPPY_KEYCHAIN" | awk '/SHA-1/ { print $3 }'
}
