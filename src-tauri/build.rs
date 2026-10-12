use std::path::PathBuf;

fn main() {
    bundle_pdfium();
    tauri_build::build()
}

/// Copy the PDFium shared library that liteparse-pdfium-sys downloaded into
/// `resources/`, so Tauri bundles it with the app (loaded at startup by
/// `document::liteparse::configure_library_dir`). Must run before
/// `tauri_build::build()`, which checks that bundled resources exist.
fn bundle_pdfium() {
    println!("cargo:rerun-if-changed=build.rs");
    let Ok(lib_dir) = std::env::var("DEP_PDFIUM_LIB_PATH") else {
        println!("cargo:warning=DEP_PDFIUM_LIB_PATH not set; PDFium will not be bundled");
        return;
    };
    let lib_dir = PathBuf::from(lib_dir);
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let (name, src_dir) = match target_os.as_str() {
        // pdfium-binaries ships the Windows DLL in bin/, next to lib/.
        "windows" => ("pdfium.dll", lib_dir.parent().map(|p| p.join("bin")).unwrap_or(lib_dir)),
        "macos" => ("libpdfium.dylib", lib_dir),
        _ => ("libpdfium.so", lib_dir),
    };

    let dest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    let src = src_dir.join(name);
    let dest = dest_dir.join(name);
    if let Err(e) = std::fs::create_dir_all(&dest_dir).and_then(|_| std::fs::copy(&src, &dest)) {
        println!("cargo:warning=could not bundle {}: {}", src.display(), e);
    }
}
