//! Insert text into the focused application.
//!
//! Every synthetic keystroke is posted from the main thread. On macOS the keyboard
//! layout lookup behind non-Unicode keys (HIToolbox TIS, used for the ⌘V paste
//! shortcut) asserts that it runs on the main queue; calling it from a worker
//! thread kills the process with EXC_BREAKPOINT.

use crate::settings::InsertMethod;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::mpsc;
use std::time::Duration;
use tauri::AppHandle;

const TYPE_MAX_CHARS: usize = 220;
/// How long to wait for the UI thread to post the keystrokes before giving up.
const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(8);

pub fn insert_text(app: &AppHandle, text: &str, method: InsertMethod) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    // macOS drops posted keystrokes silently when the app is not trusted for Accessibility,
    // so check up front and surface a real error instead of a fake success.
    #[cfg(target_os = "macos")]
    if crate::permissions::check().accessibility == Some(false) {
        static PROMPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !PROMPTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::permissions::request(crate::permissions::PermissionKind::Accessibility);
        }
        anyhow::bail!("Accessibility permission missing (System Settings → Privacy & Security → Accessibility)");
    }
    let method = match method {
        InsertMethod::Auto => {
            if text.chars().count() <= TYPE_MAX_CHARS && !text.contains('\n') {
                InsertMethod::Type
            } else {
                InsertMethod::Paste
            }
        }
        m => m,
    };
    match method {
        InsertMethod::Type => {
            let text = text.to_string();
            on_main_thread(app, move || type_text(&text))
        }
        _ => paste_text(app, text),
    }
}

fn new_enigo() -> anyhow::Result<Enigo> {
    let settings = Settings { release_keys_when_dropped: true, ..Settings::default() };
    Enigo::new(&settings).map_err(|e| anyhow::anyhow!("input simulation unavailable: {e}"))
}

fn type_text(text: &str) -> anyhow::Result<()> {
    let mut enigo = new_enigo()?;
    enigo.text(text).map_err(|e| anyhow::anyhow!("typing failed: {e}"))?;
    Ok(())
}

fn paste_text(app: &AppHandle, text: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard unavailable: {e}"))?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).map_err(|e| anyhow::anyhow!("clipboard write failed: {e}"))?;
    drop(clipboard);
    std::thread::sleep(Duration::from_millis(40));

    on_main_thread(app, paste_keystroke)?;

    // Give the target app time to read the clipboard before restoring it.
    if let Some(prev) = previous {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(prev);
            }
        });
    }
    Ok(())
}

fn paste_keystroke() -> anyhow::Result<()> {
    let mut enigo = new_enigo()?;
    let modifier = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
    enigo.key(modifier, Direction::Press).map_err(|e| anyhow::anyhow!("{e}"))?;
    let r = enigo.key(Key::Unicode('v'), Direction::Click);
    let _ = enigo.key(modifier, Direction::Release);
    r.map_err(|e| anyhow::anyhow!("paste keystroke failed: {e}"))?;
    Ok(())
}

/// Run `f` where input may be posted and wait for its result. On macOS that is the
/// main thread (dispatched through the Tauri event loop unless we are already on
/// it); elsewhere input APIs are thread-agnostic and `f` runs inline.
pub fn on_main_thread<T, F>(app: &AppHandle, f: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    if !needs_main_thread_dispatch() {
        return f();
    }
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(|e| anyhow::anyhow!("could not reach the UI thread: {e}"))?;
    match rx.recv_timeout(MAIN_THREAD_TIMEOUT) {
        Ok(r) => r,
        Err(mpsc::RecvTimeoutError::Timeout) => anyhow::bail!("the UI thread did not respond in time"),
        Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("input task was dropped before it ran"),
    }
}

#[cfg(target_os = "macos")]
fn needs_main_thread_dispatch() -> bool {
    extern "C" {
        fn pthread_main_np() -> std::os::raw::c_int;
    }
    // SAFETY: plain libc query with no preconditions.
    unsafe { pthread_main_np() == 0 }
}

#[cfg(not(target_os = "macos"))]
fn needs_main_thread_dispatch() -> bool {
    false
}

pub fn copy_text(text: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard unavailable: {e}"))?;
    clipboard.set_text(text).map_err(|e| anyhow::anyhow!("clipboard write failed: {e}"))?;
    Ok(())
}
