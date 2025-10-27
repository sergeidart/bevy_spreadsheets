// build.rs
// Embed Windows application icon into the produced .exe using the embed-resource crate.

fn main() {
    #[cfg(windows)]
    {
        use embed_resource::CompilationResult;
        use std::{env, path::PathBuf};

        // We prefer generating a multi‑size ICO from PNG to ensure proper
        // appearance in taskbar, Start menu, and Explorer at all DPIs.
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let assets_dir = PathBuf::from(&manifest_dir).join("assets");
        let png_abs = assets_dir.join("icon.png");
        let ico_abs = assets_dir.join("icon.ico");

        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let generated_ico = out_dir.join("app_icon_multi.ico");
        let rc_path = out_dir.join("app_icon.rc");

        // Helper: write RC pointing at provided ICO file path
        fn write_and_compile_rc(ico_path: &PathBuf, rc_path: &PathBuf) {
            // RC.EXE runs through a C preprocessor; unescaped backslashes in Windows paths
            // would be interpreted as escapes (e.g., \t, \b), corrupting the path. Escape them.
            let mut path_str = ico_path.to_string_lossy().into_owned();
            path_str = path_str.replace('\\', "\\\\");
            let rc_contents = format!("1 ICON \"{}\"\n", path_str);
            std::fs::write(rc_path, rc_contents).expect("Failed to write temporary RC file for icon");

            // Compile the RC; treat icon embedding as optional in dev, but warn loudly when tooling is missing.
            let result = embed_resource::compile(rc_path, embed_resource::NONE);
            match &result {
                &CompilationResult::NotWindows => {
                    println!("cargo:warning=Not a Windows target; skipping icon embedding.");
                }
                &CompilationResult::Ok => {
                    // Useful when diagnosing CI differences
                    println!("cargo:warning=Windows icon resource compiled and linked successfully.");
                }
                &CompilationResult::NotAttempted(ref why) => {
                    println!(
                        "cargo:warning=Windows resource compilation not attempted: {}.\n\
                         Install Windows 10/11 SDK (RC.EXE) or LLVM RC, or set RC env var to the compiler.\n\
                         Without this, the .exe file icon cannot be embedded.",
                        why
                    );
                }
                &CompilationResult::Failed(ref err) => {
                    // Fail fast in release builds to catch packaging issues.
                    let profile = std::env::var("PROFILE").unwrap_or_default();
                    if profile.eq_ignore_ascii_case("release") {
                        panic!("Embedding Windows icon failed: {}", err);
                    } else {
                        println!("cargo:warning=Embedding Windows icon failed: {}", err);
                    }
                }
            }

            // 3.x API suggests using one of manifest_*; optional is fine for icons.
            let _ = result.manifest_optional();
        }

        // Try to generate a multi-size ICO from PNG. If that fails, fall back to assets/icon.ico if present.
        let mut used_path: Option<PathBuf> = None;
        if png_abs.exists() {
            // Generate an ICO with common sizes
            #[allow(clippy::single_match)]
            match generate_multi_size_ico_from_png(&png_abs, &generated_ico) {
                Ok(_) => {
                    println!("cargo:rerun-if-changed={}", png_abs.display());
                    used_path = Some(generated_ico.clone());
                }
                Err(e) => {
                    println!(
                        "cargo:warning=Failed to generate multi-size ICO from {}: {}",
                        png_abs.display(),
                        e
                    );
                }
            }
        }

        if used_path.is_none() && ico_abs.exists() {
            println!("cargo:rerun-if-changed={}", ico_abs.display());
            used_path = Some(ico_abs.clone());
        }

        if let Some(ico_path) = used_path {
            write_and_compile_rc(&ico_path, &rc_path);
        } else {
            println!("cargo:warning=No icon found in assets/icon.png or assets/icon.ico; EXE icon will be generic.");
        }
    }
}

#[cfg(windows)]
fn generate_multi_size_ico_from_png(png_abs: &std::path::Path, out_ico: &std::path::Path) -> Result<(), String> {
    use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
    use image::imageops::FilterType;
    use image::DynamicImage;
    use std::fs::File;

    let img = image::open(png_abs)
        .map_err(|e| format!("Failed to open PNG {}: {}", png_abs.display(), e))?;

    let mut icon = IconDir::new(ResourceType::Icon);
    // Common Windows icon sizes; include small and large for DPIs
    let sizes: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

    for size in sizes {
        let resized: DynamicImage = img.resize_exact(size, size, FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (w, h) = (rgba.width() as u32, rgba.height() as u32);
        let icon_img = IconImage::from_rgba_data(w, h, rgba.into_raw());
        let entry: IconDirEntry = IconDirEntry::encode(&icon_img)
            .map_err(|e| format!("Failed to encode ICO entry {size}x{size}: {e}"))?;
        icon.add_entry(entry);
    }

    let file = File::create(out_ico)
        .map_err(|e| format!("Failed to create {}: {}", out_ico.display(), e))?;
    icon.write(file)
        .map_err(|e| format!("Failed to write {}: {}", out_ico.display(), e))?
        ;
    Ok(())
}
