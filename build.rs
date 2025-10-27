// build.rs
// Embed Windows application icon into the produced .exe using the embed-resource crate.

fn main() {
    #[cfg(windows)]
    {
        use std::{env, fs, path::PathBuf};
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let ico_abs: PathBuf = PathBuf::from(&manifest_dir).join("assets").join("icon.ico");
        if ico_abs.exists() {
            // Generate a temporary RC file with an absolute path to avoid relative path issues
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            let rc_path = out_dir.join("app_icon.rc");
            let rc_contents = format!("1 ICON \"{}\"\n", ico_abs.display());
            fs::write(&rc_path, rc_contents).expect("Failed to write temporary RC file for icon");
            // Compile the temporary RC file. Pass an explicitly typed empty iterator
            // for defines to satisfy generics.
            embed_resource::compile(&rc_path, std::iter::empty::<&str>());
            println!("cargo:rerun-if-changed={}", ico_abs.display());
        } else {
            println!("cargo:warning=assets/icon.ico not found; EXE icon will be generic. Runtime window icon still set from PNG.");
        }
    }
}
