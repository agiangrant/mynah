fn main() {
    link_apple_compiler_rt();
    tauri_build::build()
}

/// whisper.cpp's Metal backend uses `@available()` checks. With a deployment target
/// below the checked version clang lowers those to `___isPlatformVersionAtLeast`,
/// which lives in clang's compiler-rt (`libclang_rt.osx.a`). rustc links with
/// `-nodefaultlibs`, so the archive has to be added by hand.
fn link_apple_compiler_rt() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(out) = std::process::Command::new("xcrun").args(["clang", "-print-resource-dir"]).output() else {
        return;
    };
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let lib_dir = std::path::Path::new(&dir).join("lib").join("darwin");
    if lib_dir.join("libclang_rt.osx.a").exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    } else {
        println!("cargo:warning=libclang_rt.osx.a not found under {}", lib_dir.display());
    }
}
