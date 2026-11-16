#!/bin/bash
# Set Snappy's version everywhere it's written down, ready for a release:
#
#   tools/release/bump.sh patch     # 0.1.0 → 0.1.1
#   tools/release/bump.sh minor     # 0.1.0 → 0.2.0
#   tools/release/bump.sh major     # 0.1.0 → 1.0.0
#   tools/release/bump.sh 0.3.0     # or any newer version
#
# The workspace's Cargo.toml and the workspace crates in Cargo.lock, the macOS property lists,
# the GNOME extension's metadata, and the app's package.json. Tests check they all match.
# It changes files only; committing, tagging, and pushing are left to you.
# The awk programs are in single quotes so the shell leaves their $ alone.
# shellcheck disable=SC2016
set -euo pipefail

# shellcheck source=tools/release/common.sh
source "$(dirname "$0")/common.sh"
cd "$release_root"

usage="usage: tools/release/bump.sh patch|minor|major|<version>, such as 0.1.1"
[[ $# -eq 1 ]] || die "$usage"

[[ -z "$(git status --porcelain)" ]] || die "the working tree has changes; commit or set them aside first"

current="$(snappy_version)"
[[ -n "$current" ]] || die "can't find the version in Cargo.toml"
IFS=. read -r major minor patch <<<"$current"

case "$1" in
    patch) version="$major.$minor.$((patch + 1))" ;;
    minor) version="$major.$((minor + 1)).0" ;;
    major) version="$((major + 1)).0.0" ;;
    *) version="${1#v}" ;;
esac
# Plain MAJOR.MINOR.PATCH: Windows Installer and macOS bundle versions are numbers only, so
# pre-release suffixes can't be carried everywhere.
number='(0|[1-9][0-9]*)'
[[ "$version" =~ ^$number\.$number\.$number$ ]] ||
    die "'$1' isn't patch, minor, major, or a version like 1.2.3 (no pre-release or build suffix)"
[[ "$version" != "$current" ]] || die "the version is already $current"
IFS=. read -r -a old <<<"$current"
IFS=. read -r -a new <<<"$version"
for i in 0 1 2; do
    if ((new[i] > old[i])); then
        break
    elif ((new[i] < old[i])); then
        die "$version is older than the current version, $current"
    fi
done

# The tree was clean, so whatever stopped the bump partway is undone by a checkout.
trap '(($? == 0)) || echo "git checkout -- . undoes the files changed so far." >&2' EXIT

# Rewrites a file through an awk program given the old and new versions, keeping its mode.
rewrite() {
    local file="$1" program="$2"
    awk -v old="$current" -v new="$version" "$program" "$file" >"$file.bump"
    if cmp -s "$file" "$file.bump"; then
        rm "$file.bump"
        die "$file doesn't hold the version $current where it was expected"
    fi
    cat "$file.bump" >"$file"
    rm "$file.bump"
}

# The only version in the workspace manifest is in [workspace.package]; crates inherit it.
rewrite Cargo.toml '
    /^\[/ { in_package = ($0 == "[workspace.package]") }
    in_package && $0 == "version = \"" old "\"" { $0 = "version = \"" new "\"" }
    { print }'

# Workspace crates are the lock file entries without a `source`, which comes after `version`,
# so each entry is held until it ends.
rewrite Cargo.lock '
    function flush(   i) {
        for (i = 1; i <= count; i++) {
            if (!sourced && entry[i] == "version = \"" old "\"") entry[i] = "version = \"" new "\""
            print entry[i]
        }
        count = 0; sourced = 0
    }
    /^\[\[package\]\]$/ || /^$/ { flush() }
    /^source = / { sourced = 1 }
    { entry[++count] = $0 }
    END { flush() }'

for plist in packaging/macos/Snappy.plist packaging/macos/SnappyApp.plist; do
    rewrite "$plist" '
        versioned && $0 == "\t<string>" old "</string>" { $0 = "\t<string>" new "</string>" }
        { versioned = ($0 ~ /<key>CFBundle(ShortVersionString|Version)<\/key>/); print }'
done

rewrite crates/gnome/extension/metadata.json '
    $0 ~ /^  "version-name": / { sub("\"" old "\"", "\"" new "\"") }
    { print }'

rewrite apps/app/ui/package.json '
    $0 ~ /^  "version": / { sub("\"" old "\"", "\"" new "\"") }
    { print }'

# The lock file must still match the manifests.
if ! problem="$(cargo metadata --locked --format-version 1 2>&1 >/dev/null)"; then
    echo "$problem" >&2
    die "Cargo.lock no longer matches Cargo.toml"
fi

echo "Snappy is now $version (was $current):"
git status --short
cat <<EOF

Next:
  1. Check it:   tools/check.sh
  2. Commit it:  git commit -am "Release $version"
  3. Tag it:     git tag -a v$version -m "Snappy $version"
  4. Push both:  git push origin HEAD v$version

Pushing the tag starts the release workflow, which builds every package and drafts a
GitHub release. docs/releasing/README.md has the rest.
EOF
