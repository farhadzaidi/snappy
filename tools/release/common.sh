# shellcheck shell=bash
# Shared by the release scripts. Sourced, not run.

release_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# Where the release scripts put the finished packages.
# shellcheck disable=SC2034
release_out="$release_root/dist/release"

# Snappy's version: `version` in `[workspace.package]` of the workspace's Cargo.toml.
snappy_version() {
    awk '
        /^\[/ { in_package = ($0 == "[workspace.package]") }
        in_package && /^version *=/ {
            sub(/^version *= *"/, ""); sub(/".*/, ""); print; exit
        }
    ' "$release_root/Cargo.toml"
}

# A field of `[workspace.package]`, such as `license`, or nothing when it isn't set.
workspace_field() {
    awk -v field="$1" '
        /^\[/ { in_package = ($0 == "[workspace.package]") }
        in_package && $1 == field && $2 == "=" {
            sub(/^[^=]*= *"/, ""); sub(/".*/, ""); print; exit
        }
    ' "$release_root/Cargo.toml"
}

die() {
    echo "error: $*" >&2
    exit 1
}
