mpv_version="0.42.0"
mpv_source_checksum="bcfebbfa2b65a6d88dc97ea18117831072373e38b5785a2afb874ed4df5070dd"

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
