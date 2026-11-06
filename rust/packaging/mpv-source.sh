mpv_version="0.41.0"
mpv_source_checksum="ee21092a5ee427353392360929dc64645c54479aefdb5babc5cfbb5fad626209"

download_verified() {
    local url="$1" checksum="$2" output="$3"
    curl --fail --location --silent --show-error --proto '=https' --output "$output" "$url"
    echo "$checksum  $output" | sha256sum --check --quiet
}

extract_mpv_source() {
    local work_dir="$1"
    download_verified "https://github.com/mpv-player/mpv/archive/refs/tags/v$mpv_version.tar.gz" \
        "$mpv_source_checksum" "$work_dir/mpv.tar.gz"
    tar -xzf "$work_dir/mpv.tar.gz" -C "$work_dir"
}
