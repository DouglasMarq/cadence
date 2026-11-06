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

libplacebo_version="7.387.0"
libplacebo_checksum="f9e62d3774a9c4cfd198ec3abaa88294aaade2e31c04d5ab02bcf4abc98b1df5"
work_dir="$(mktemp -d -t cadence-libmpv.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT

build_libplacebo() {
    download_verified \
        "https://github.com/haasn/libplacebo/archive/refs/tags/v$libplacebo_version.tar.gz" \
        "$libplacebo_checksum" "$work_dir/libplacebo.tar.gz"
    tar -xzf "$work_dir/libplacebo.tar.gz" -C "$work_dir"
    meson setup "$work_dir/libplacebo-build" "$work_dir/libplacebo-$libplacebo_version" \
        --buildtype=release -Ddemos=false -Dvulkan=disabled -Dopengl=disabled -Dd3d11=disabled \
        -Dshaderc=disabled -Dglslang=disabled
    meson compile -C "$work_dir/libplacebo-build"
    sudo meson install -C "$work_dir/libplacebo-build" --no-rebuild
    sudo ldconfig
}

build_mpv() {
    extract_mpv_source "$work_dir"
    meson setup "$work_dir/build" "$work_dir/mpv-$mpv_version" --buildtype=release \
        -Dlibmpv=true -Dcplayer=false \
        -Dalsa=enabled -Djack=enabled -Dlibavdevice=enabled -Dlua=enabled -Dpipewire=enabled \
        -Dpulse=enabled
    meson compile -C "$work_dir/build"
    sudo meson install -C "$work_dir/build" --no-rebuild
    sudo ldconfig
}

build_libplacebo
build_mpv
