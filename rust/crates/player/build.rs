use std::path::Path;

const HOMEBREW_LIBRARY_DIRECTORIES: [&str; 2] = ["/opt/homebrew/lib", "/usr/local/lib"];
const WINDOWS_LIBRARY_VARIABLE: &str = "CADENCE_LIBMPV_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed={WINDOWS_LIBRARY_VARIABLE}");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => link_homebrew_mpv(),
        Ok("windows") => link_downloaded_mpv(),
        _ => {}
    }
}

fn link_homebrew_mpv() {
    let library_directories = match pkg_config::Config::new().cargo_metadata(false).probe("mpv") {
        Ok(library) => library.link_paths,
        Err(_) => HOMEBREW_LIBRARY_DIRECTORIES
            .iter()
            .map(Path::new)
            .filter(|directory| directory.join("libmpv.dylib").exists())
            .map(Path::to_path_buf)
            .collect(),
    };
    for directory in library_directories {
        println!("cargo:rustc-link-search=native={}", directory.display());
    }
}

fn link_downloaded_mpv() {
    if let Some(directory) = std::env::var_os(WINDOWS_LIBRARY_VARIABLE) {
        println!(
            "cargo:rustc-link-search=native={}",
            Path::new(&directory).display()
        );
    }
}
