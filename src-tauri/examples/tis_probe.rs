//! Reproduces and verifies the ⌘V crash: inside a running Tauri/AppKit app, the
//! keyboard-layout lookup enigo performs for `Key::Unicode('v')` (HIToolbox TIS)
//! must run on the main thread.
//!
//!   cargo run --release --example tis_probe -- thread   # what the app used to do (crashes with SIGTRAP)
//!   cargo run --release --example tis_probe -- main     # dispatched like inject.rs does now
//!
//! Exit code 0 = lookup survived.

use std::ffi::c_void;
use std::time::Duration;

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(src: *mut c_void, key: *const c_void) -> *mut c_void;
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
}

#[cfg(target_os = "macos")]
fn lookup() -> anyhow::Result<bool> {
    // SAFETY: plain Carbon calls; the returned refs are only null-checked.
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        Ok(!src.is_null() && !TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData).is_null())
    }
}

#[cfg(not(target_os = "macos"))]
fn lookup() -> anyhow::Result<bool> {
    Ok(true)
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "thread".into());
    tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("probe-worker".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_millis(800));
                    let result = if mode == "main" {
                        mynah_lib::inject::on_main_thread(&handle, lookup)
                    } else {
                        lookup()
                    };
                    println!("probe[{mode}]: layout lookup -> {result:?}");
                    std::process::exit(if result.is_ok() { 0 } else { 2 });
                })
                .expect("spawn probe");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("tauri run");
}
