use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use slint_build::{CompilerConfiguration, DefaultTranslationContext};

const TRANSLATIONS_DIR: &str = "translations";
const TRANSLATION_DOMAIN: &str = "cadence";
const ICONS_DIR: &str = "../../assets/icons";
const WINDOWS_ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
const WINDOWS_ICON_FILE: &str = "cadence.ico";
const ICO_RESERVED: u16 = 0;
const ICO_TYPE_ICON: u16 = 1;
const ICO_HEADER_LENGTH: usize = 6;
const ICO_ENTRY_LENGTH: usize = 16;
const ICO_DIMENSION_OF_256: u8 = 0;
const ICO_NO_PALETTE: [u8; 2] = [0, 0];
const ICO_PLANES: u16 = 1;
const ICO_BITS_PER_PIXEL: u16 = 32;

struct IconImage {
    size: u32,
    png: Vec<u8>,
}

fn main() -> Result<(), Box<dyn Error>> {
    compile_ui()?;
    write_catalogs(Path::new(TRANSLATIONS_DIR))?;
    println!("cargo:rerun-if-changed={TRANSLATIONS_DIR}");
    embed_windows_resources()?;
    Ok(())
}

fn embed_windows_resources() -> Result<(), Box<dyn Error>> {
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target_os| target_os != "windows") {
        return Ok(());
    }
    let icon = PathBuf::from(std::env::var("OUT_DIR")?).join(WINDOWS_ICON_FILE);
    std::fs::write(&icon, windows_icon(&icon_images()?)?)?;
    let icon = icon
        .to_str()
        .ok_or("the build directory is not valid UTF-8")?;
    winresource::WindowsResource::new()
        .set_icon(icon)
        .compile()?;
    Ok(())
}

fn icon_images() -> Result<Vec<IconImage>, Box<dyn Error>> {
    WINDOWS_ICON_SIZES
        .iter()
        .map(|size| {
            let png = Path::new(ICONS_DIR).join(format!("{size}x{size}.png"));
            println!("cargo:rerun-if-changed={}", png.display());
            Ok(IconImage {
                size: *size,
                png: std::fs::read(png)?,
            })
        })
        .collect()
}

fn windows_icon(images: &[IconImage]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut icon = Vec::new();
    icon.extend(ICO_RESERVED.to_le_bytes());
    icon.extend(ICO_TYPE_ICON.to_le_bytes());
    icon.extend(u16::try_from(images.len())?.to_le_bytes());
    let mut image_offset = ICO_HEADER_LENGTH + ICO_ENTRY_LENGTH * images.len();
    for image in images {
        icon.extend(icon_entry(image, image_offset)?);
        image_offset += image.png.len();
    }
    for image in images {
        icon.extend(&image.png);
    }
    Ok(icon)
}

fn icon_entry(image: &IconImage, image_offset: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let dimension = u8::try_from(image.size).unwrap_or(ICO_DIMENSION_OF_256);
    let mut entry = vec![dimension, dimension];
    entry.extend(ICO_NO_PALETTE);
    entry.extend(ICO_PLANES.to_le_bytes());
    entry.extend(ICO_BITS_PER_PIXEL.to_le_bytes());
    entry.extend(u32::try_from(image.png.len())?.to_le_bytes());
    entry.extend(u32::try_from(image_offset)?.to_le_bytes());
    Ok(entry)
}

fn compile_ui() -> Result<(), Box<dyn Error>> {
    let debug_build = std::env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    let config = CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_debug_info(debug_build)
        .with_bundled_translations(TRANSLATIONS_DIR)
        .with_default_translation_context(DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).map_err(|error| error.to_string())?;
    Ok(())
}

fn catalog_files(translations_dir: &Path) -> Result<Vec<(String, PathBuf)>, Box<dyn Error>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(translations_dir)? {
        let language_dir = entry?.path();
        let catalog = language_dir
            .join("LC_MESSAGES")
            .join(format!("{TRANSLATION_DOMAIN}.po"));
        let language = language_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        if let (Some(language), true) = (language, catalog.exists()) {
            println!("cargo:rerun-if-changed={}", catalog.display());
            files.push((language, catalog));
        }
    }
    files.sort();
    Ok(files)
}

fn translated_forms(entry: &rspolib::POEntry) -> Option<Vec<String>> {
    let forms = match entry.msgid_plural {
        Some(_) => entry.msgstr_plural.clone(),
        None => entry.msgstr.clone().into_iter().collect(),
    };
    let complete = !entry.msgid.is_empty()
        && !entry.obsolete
        && !forms.is_empty()
        && forms.iter().all(|form| !form.is_empty());
    complete.then_some(forms)
}

fn catalog_source(language: &str, catalog: &Path) -> Result<String, Box<dyn Error>> {
    let file = rspolib::pofile(catalog).map_err(|error| error.to_string())?;
    let mut source = format!("    super::Catalog {{ language: {language:?}, messages: &[\n");
    for entry in &file.entries {
        let Some(forms) = translated_forms(entry) else {
            continue;
        };
        let plural = entry.msgid_plural.clone().unwrap_or_default();
        writeln!(
            source,
            "        super::Message {{ singular: {:?}, plural: {plural:?}, forms: &{forms:?} }},",
            entry.msgid
        )?;
    }
    source.push_str("    ] },\n");
    Ok(source)
}

fn write_catalogs(translations_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut source = String::from("pub static CATALOGS: &[super::Catalog] = &[\n");
    for (language, catalog) in catalog_files(translations_dir)? {
        source.push_str(&catalog_source(&language, &catalog)?);
    }
    source.push_str("];\n");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("catalogs.rs"), source)?;
    Ok(())
}
