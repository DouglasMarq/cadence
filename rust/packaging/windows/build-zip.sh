#!/usr/bin/env bash
set -euo pipefail
shopt -s inherit_errexit

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rust_dir="$(cd "$script_dir/../.." && pwd)"
out_dir="${OUT_DIR:-$rust_dir/target/windows}"
libmpv_dir="${CADENCE_LIBMPV_DIR:?set CADENCE_LIBMPV_DIR to the folder written by fetch-libmpv.sh}"

for tool in cargo 7z; do
    command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$rust_dir/Cargo.toml" | head -n1)"
arch="$(uname -m)"

(cd "$rust_dir" && cargo build --release -p cadence)

package_dir="$(mktemp -d -t cadence-windows.XXXXXX)"
trap 'rm -rf "$package_dir"' EXIT
cp "$rust_dir/target/release/cadence.exe" "$libmpv_dir/libmpv-2.dll" "$package_dir/"

mkdir -p "$out_dir"
archive="$(cd "$out_dir" && pwd)/Cadence-$version-windows-$arch.zip"
rm -f "$archive"
(cd "$package_dir" && 7z a -tzip -mx=9 "$archive" cadence.exe libmpv-2.dll >/dev/null)
echo "$archive"
