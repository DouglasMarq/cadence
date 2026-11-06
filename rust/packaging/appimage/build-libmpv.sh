#!/usr/bin/env bash
# Usage: build-libmpv.sh
# Builds libmpv and installs it into /usr/local. Ubuntu 24.04 ships mpv 0.37, but crossfade needs
# the volume-gain property of mpv 0.38 or newer. Everything Cadence relies on is required rather
# than detected, so that a missing package fails here and not in an AppImage without sound: the
# audio outputs, Lua (the ytdl option the player disables only exists with it) and libavdevice
# (the playback tests synthesize audio through its lavfi input).
set -euo pipefail
shopt -s inherit_errexit

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/../mpv-source.sh"

work_dir="$(mktemp -d -t cadence-libmpv.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT

extract_mpv_source "$work_dir"

meson setup "$work_dir/build" "$work_dir/mpv-$mpv_version" --buildtype=release \
    -Dlibmpv=true -Dcplayer=false \
    -Dalsa=enabled -Djack=enabled -Dlibavdevice=enabled -Dlua=enabled -Dpipewire=enabled \
    -Dpulse=enabled
meson compile -C "$work_dir/build"
sudo meson install -C "$work_dir/build" --no-rebuild
sudo ldconfig
