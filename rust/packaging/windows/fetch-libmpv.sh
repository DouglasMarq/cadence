#!/usr/bin/env bash
set -euo pipefail
shopt -s inherit_errexit

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/../mpv-source.sh"

package="libmpv-v$mpv_version-x86_64-w64-mingw32-lgpl.zip"
checksum="8e5f77430efc53ef175804ba6618527e3af0448a0dcabf04d3cc72659d1a6fce"
destination="${1:?usage: fetch-libmpv.sh <directory>}"

command -v 7z >/dev/null || { echo "missing tool: 7z" >&2; exit 1; }

work_dir="$(mktemp -d -t cadence-libmpv.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT

download_verified "https://github.com/mpv-player/mpv/releases/download/v$mpv_version/$package" \
    "$checksum" "$work_dir/$package"

mkdir -p "$destination"
(cd "$destination" && 7z x -y "$work_dir/$package" libmpv-2.dll libmpv.dll.a >/dev/null)
cp "$destination/libmpv.dll.a" "$destination/mpv.lib"
cd "$destination" && pwd
