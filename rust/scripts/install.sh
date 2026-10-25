#!/usr/bin/env bash
# Usage: install.sh [--uninstall]
# Builds the release binary and installs it for the current user with an app menu entry.
# Install root defaults to ~/.local; override with PREFIX=/some/dir.
set -euo pipefail

prefix="${PREFIX:-$HOME/.local}"
rust_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
desktop_integration="$rust_dir/packaging/install-desktop-integration.sh"
binary="$prefix/bin/cadence"

refresh_menus() {
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$prefix/share/applications" || true
    [[ -f "$prefix/share/icons/hicolor/icon-theme.cache" ]] && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
}

uninstall() {
    rm -f "$binary"
    bash "$desktop_integration" --uninstall "$prefix"
    refresh_menus
    echo "Removed Cadence from $prefix (settings in ~/.config/cadence were kept)"
}

install_app() {
    cargo build --release --manifest-path "$rust_dir/Cargo.toml" -p cadence
    install -Dm755 "$rust_dir/target/release/cadence" "$binary"
    bash "$desktop_integration" "$prefix" "$binary"
    refresh_menus
    echo "Installed $binary"
}

case "${1:-}" in
    "") install_app ;;
    --uninstall) uninstall ;;
    *) echo "usage: install.sh [--uninstall]" >&2; exit 1 ;;
esac
