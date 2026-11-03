#!/usr/bin/env bash
set -euo pipefail
shopt -s inherit_errexit

package="mpv-dev-x86_64-20261101-git-48fca90e8c.7z"
checksum="30d3f9fa121ce24638610a9ae52f8957743f9dea416dd37dc4ff7e43589ef843"
destination="${1:?usage: fetch-libmpv.sh <directory>}"

command -v 7z >/dev/null || { echo "missing tool: 7z" >&2; exit 1; }

work_dir="$(mktemp -d -t cadence-libmpv.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT

curl --fail --location --silent --show-error --proto '=https' --output "$work_dir/$package" \
    "https://sourceforge.net/projects/mpv-player-windows/files/libmpv/$package/download"
echo "$checksum  $work_dir/$package" | sha256sum --check --quiet

mkdir -p "$destination"
(cd "$destination" && 7z x -y "$work_dir/$package" libmpv-2.dll libmpv.dll.a >/dev/null)
cp "$destination/libmpv.dll.a" "$destination/mpv.lib"
cd "$destination" && pwd
