# Shared by live.sh and test.sh, which source it: the GNOME versions the suites run, and the
# container that builds the binaries under test.

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/../build/build.sh"

# The image each GNOME version comes from.
declare -A bases=([46]=ubuntu:24.04 [47]=fedora:41 [48]=fedora:42 [49]=fedora:43)

# Builds the GNOME test image for a version and prints its tag, or explains and fails.
gnome_image() {
    local version="$1" base="${bases[$1]:-}"
    if [[ -z "$base" ]]; then
        echo "No image for GNOME $version; choose from ${!bases[*]}." >&2
        return 2
    fi
    docker build --quiet --tag "snappy-gnome:$version" --build-arg "BASE=$base" "$here" >/dev/null
    echo "snappy-gnome:$version"
}
