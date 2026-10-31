#!/usr/bin/env bash
# Usage: build-libmpv.sh
# Builds libmpv and installs it into /usr/local. Ubuntu 24.04 ships mpv 0.37, but crossfade needs
# the volume-gain property of mpv 0.38 or newer. Everything Cadence relies on is required rather
# than detected, so that a missing package fails here and not in an AppImage without sound: the
# audio outputs, Lua (the ytdl option the player disables only exists with it) and libavdevice
# (the playback tests synthesize audio through its lavfi input).
set -euo pipefail
shopt -s inherit_errexit

version="0.41.0"
checksum="ee21092a5ee427353392360929dc64645c54479aefdb5babc5cfbb5fad626209"
work_dir="$(mktemp -d -t cadence-libmpv.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT

curl --fail --location --silent --show-error --proto '=https' --output "$work_dir/mpv.tar.gz" \
    "https://github.com/mpv-player/mpv/archive/refs/tags/v$version.tar.gz"
echo "$checksum  $work_dir/mpv.tar.gz" | sha256sum --check --quiet
tar -xzf "$work_dir/mpv.tar.gz" -C "$work_dir"

meson setup "$work_dir/build" "$work_dir/mpv-$version" --buildtype=release \
    -Dlibmpv=true -Dcplayer=false \
    -Dalsa=enabled -Djack=enabled -Dlibavdevice=enabled -Dlua=enabled -Dpipewire=enabled \
    -Dpulse=enabled
meson compile -C "$work_dir/build"
sudo meson install -C "$work_dir/build" --no-rebuild
sudo ldconfig
