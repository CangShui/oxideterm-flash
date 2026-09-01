use std::path::PathBuf;

use oxideterm_settings::AppIconVariant;

pub(crate) const APP_ICON_VARIANTS: &[AppIconVariant] = &[
    AppIconVariant::Default,
    AppIconVariant::WhiteBlue,
    AppIconVariant::WhiteGraphite,
    AppIconVariant::WhiteGreen,
    AppIconVariant::WhitePurple,
    AppIconVariant::WhiteRed,
    AppIconVariant::FilledOrange,
    AppIconVariant::FilledBlue,
    AppIconVariant::FilledGraphite,
    AppIconVariant::FilledGreen,
    AppIconVariant::FilledPurple,
    AppIconVariant::FilledRed,
];

pub(crate) fn app_icon_variant_file_name(variant: AppIconVariant) -> &'static str {
    match variant {
        AppIconVariant::Default => "default.png",
        AppIconVariant::WhiteBlue => "white-blue.png",
        AppIconVariant::WhiteGraphite => "white-graphite.png",
        AppIconVariant::WhiteGreen => "white-green.png",
        AppIconVariant::WhitePurple => "white-purple.png",
        AppIconVariant::WhiteRed => "white-red.png",
        AppIconVariant::FilledOrange => "filled-orange.png",
        AppIconVariant::FilledBlue => "filled-blue.png",
        AppIconVariant::FilledGraphite => "filled-graphite.png",
        AppIconVariant::FilledGreen => "filled-green.png",
        AppIconVariant::FilledPurple => "filled-purple.png",
        AppIconVariant::FilledRed => "filled-red.png",
    }
}

#[cfg(target_os = "windows")]
fn app_icon_variant_ico_file_name(variant: AppIconVariant) -> String {
    app_icon_variant_file_name(variant).replace(".png", ".ico")
}

pub(crate) fn app_icon_variant_resource_path(variant: AppIconVariant) -> PathBuf {
    let file_name = app_icon_variant_file_name(variant);
    for root in app_icon_resource_roots() {
        let candidate = root.join("variants").join(&file_name);
        if candidate.exists() {
            return candidate;
        }
    }

    // Packaged apps and detached launches have no repo-relative resources;
    // fall back to the binary-embedded copy materialized into a cache file.
    match materialized_icon_cache_path(&file_name, app_icon_variant_png(variant)) {
        Ok(path) => path,
        // Cache writes are best effort; degrade to the legacy relative path so
        // callers keep their existing missing-file handling.
        Err(_) => PathBuf::from("crates")
            .join("oxideterm-gpui-app")
            .join("resources")
            .join("icons")
            .join("variants")
            .join(file_name),
    }
}

// Icon assets ship inside the binary, so runtime switching never depends on
// the working directory; the cache copy exists only for file-path loaders
// such as Win32 LoadImageW and GPUI's img element.
fn materialized_icon_cache_path(file_name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("oxideterm-app-icons");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(file_name);
    // Rewrite when missing or size-changed so updated bundled assets propagate.
    let stale = std::fs::metadata(&path)
        .map(|meta| meta.len() as usize != bytes.len())
        .unwrap_or(true);
    if stale {
        std::fs::write(&path, bytes)?;
    }
    Ok(path)
}

fn app_icon_resource_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        roots.push(exe_dir.join("resources").join("icons"));
        roots.push(exe_dir.join("..").join("Resources").join("icons"));
        roots.push(exe_dir.join("icons"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(
            cwd.join("crates")
                .join("oxideterm-gpui-app")
                .join("resources")
                .join("icons"),
        );
    }
    roots
}

fn app_icon_variant_png(variant: AppIconVariant) -> &'static [u8] {
    match variant {
        AppIconVariant::Default => include_bytes!("../resources/icons/variants/default.png"),
        AppIconVariant::WhiteBlue => include_bytes!("../resources/icons/variants/white-blue.png"),
        AppIconVariant::WhiteGraphite => {
            include_bytes!("../resources/icons/variants/white-graphite.png")
        }
        AppIconVariant::WhiteGreen => {
            include_bytes!("../resources/icons/variants/white-green.png")
        }
        AppIconVariant::WhitePurple => {
            include_bytes!("../resources/icons/variants/white-purple.png")
        }
        AppIconVariant::WhiteRed => include_bytes!("../resources/icons/variants/white-red.png"),
        AppIconVariant::FilledOrange => {
            include_bytes!("../resources/icons/variants/filled-orange.png")
        }
        AppIconVariant::FilledBlue => include_bytes!("../resources/icons/variants/filled-blue.png"),
        AppIconVariant::FilledGraphite => {
            include_bytes!("../resources/icons/variants/filled-graphite.png")
        }
        AppIconVariant::FilledGreen => {
            include_bytes!("../resources/icons/variants/filled-green.png")
        }
        AppIconVariant::FilledPurple => {
            include_bytes!("../resources/icons/variants/filled-purple.png")
        }
        AppIconVariant::FilledRed => include_bytes!("../resources/icons/variants/filled-red.png"),
    }
}

#[cfg(target_os = "windows")]
fn app_icon_variant_ico(variant: AppIconVariant) -> &'static [u8] {
    match variant {
        AppIconVariant::Default => include_bytes!("../resources/icons/variants/default.ico"),
        AppIconVariant::WhiteBlue => include_bytes!("../resources/icons/variants/white-blue.ico"),
        AppIconVariant::WhiteGraphite => {
            include_bytes!("../resources/icons/variants/white-graphite.ico")
        }
        AppIconVariant::WhiteGreen => include_bytes!("../resources/icons/variants/white-green.ico"),
        AppIconVariant::WhitePurple => {
            include_bytes!("../resources/icons/variants/white-purple.ico")
        }
        AppIconVariant::WhiteRed => include_bytes!("../resources/icons/variants/white-red.ico"),
        AppIconVariant::FilledOrange => {
            include_bytes!("../resources/icons/variants/filled-orange.ico")
        }
        AppIconVariant::FilledBlue => include_bytes!("../resources/icons/variants/filled-blue.ico"),
        AppIconVariant::FilledGraphite => {
            include_bytes!("../resources/icons/variants/filled-graphite.ico")
        }
        AppIconVariant::FilledGreen => {
            include_bytes!("../resources/icons/variants/filled-green.ico")
        }
        AppIconVariant::FilledPurple => {
            include_bytes!("../resources/icons/variants/filled-purple.ico")
        }
        AppIconVariant::FilledRed => include_bytes!("../resources/icons/variants/filled-red.ico"),
    }
}

#[cfg(target_os = "windows")]
fn app_icon_variant_ico_resource_path(variant: AppIconVariant) -> PathBuf {
    let file_name = app_icon_variant_ico_file_name(variant);
    for root in app_icon_resource_roots() {
        let candidate = root.join("variants").join(&file_name);
        if candidate.exists() {
            return candidate;
        }
    }

    // LoadImageW needs a file path, so materialize the embedded icon into the
    // cache directory instead of failing on repo-relative resources.
    match materialized_icon_cache_path(&file_name, app_icon_variant_ico(variant)) {
        Ok(path) => path,
        Err(_) => PathBuf::from("crates")
            .join("oxideterm-gpui-app")
            .join("resources")
            .join("icons")
            .join("variants")
            .join(file_name),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn install_runtime_app_icon(variant: AppIconVariant) {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };

    // Cargo-bundle uses the icon metadata for packaged apps; this keeps
    // development runs and runtime variants visually aligned with the setting.
    let bytes = app_icon_variant_png(variant);
    let data = unsafe { NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len()) };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("failed to decode bundled OxideTerm application icon");
        return;
    };

    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&image));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn install_runtime_app_icon(variant: AppIconVariant) {
    let icon_path = app_icon_variant_ico_resource_path(variant);
    if let Err(error) = oxideterm_desktop_presence::set_application_icon(&icon_path) {
        eprintln!("failed to apply Windows application icon: {error:#}");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn install_runtime_app_icon(_variant: AppIconVariant) {
    // Linux desktop shells resolve the installed icon through desktop metadata.
}
