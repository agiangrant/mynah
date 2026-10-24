//! macOS global key listener built directly on a CGEventTap.
//!
//! `rdev` is not used here: its macOS callback resolves the typed character through
//! HIToolbox text-input APIs, which newer macOS versions assert must run on the main
//! thread. We only need keycodes and modifier flags, so we read those and nothing else.

use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, CallbackResult,
    EventField,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

/// Without Input Monitoring a tap can still be created, but macOS only delivers this
/// process's own keystrokes. Report that clearly instead of silently half-working.
pub fn has_input_monitoring() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

/// Blocks forever. Retries until the event tap can be created (Input Monitoring granted).
/// `on_key(name, down)` receives rdev-compatible key names.
pub fn run_forever(on_key: Arc<dyn Fn(String, bool) + Send + Sync>, on_error: impl Fn(String)) {
    let mut reported = false;
    if !has_input_monitoring() {
        // Shows the system prompt / adds Mynah to the Input Monitoring list.
        let granted = unsafe { CGRequestListenEventAccess() };
        log::warn!("Input Monitoring not granted (request returned {granted}); global keys will not be seen until it is");
        on_error("Input Monitoring permission missing: Mynah only sees its own keystrokes".into());
        reported = true;
    }
    loop {
        // A tap created without permission never sees other apps' keys; wait for the grant
        // rather than installing a half-working tap.
        if !has_input_monitoring() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        match run_once(on_key.clone()) {
            Ok(()) => {
                log::warn!("key event tap run loop ended; restarting");
            }
            Err(e) => {
                if !reported {
                    on_error(e.clone());
                    log::error!("key event tap unavailable: {e} (retrying every 3s)");
                    reported = true;
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        }
    }
}

fn run_once(on_key: Arc<dyn Fn(String, bool) + Send + Sync>) -> Result<(), String> {
    let port_slot: Arc<AtomicPtr<std::ffi::c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
    let slot = port_slot.clone();
    let pressed_mods = parking_lot::Mutex::new(HashSet::<i64>::new());

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            CGEventType::TapDisabledByTimeout,
            CGEventType::TapDisabledByUserInput,
        ],
        move |_proxy, kind, event: &CGEvent| {
            match kind {
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                    let port = slot.load(Ordering::Acquire);
                    if !port.is_null() {
                        unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
                    }
                }
                CGEventType::KeyDown | CGEventType::KeyUp => {
                    let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    on_key(key_name_from_code(code), matches!(kind, CGEventType::KeyDown));
                }
                CGEventType::FlagsChanged => {
                    let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    let flags = event.get_flags().bits();
                    let down = match modifier_bit(code) {
                        Some(bit) => flags & bit != 0,
                        None => {
                            // unknown modifier: alternate press/release
                            let mut p = pressed_mods.lock();
                            if p.remove(&code) {
                                false
                            } else {
                                p.insert(code);
                                true
                            }
                        }
                    };
                    on_key(key_name_from_code(code), down);
                }
                _ => {}
            }
            CallbackResult::Keep
        },
    )
    .map_err(|_| "could not create keyboard event tap (Input Monitoring permission missing?)".to_string())?;

    port_slot.store(tap.mach_port().as_concrete_TypeRef() as *mut _, Ordering::Release);
    let source = tap
        .mach_port()
        .create_runloop_source(0)
        .map_err(|_| "could not create run loop source for event tap".to_string())?;
    let run_loop = CFRunLoop::get_current();
    unsafe { run_loop.add_source(&source, kCFRunLoopCommonModes) };
    tap.enable();
    log::info!("keyboard event tap installed");
    CFRunLoop::run_current();
    port_slot.store(std::ptr::null_mut(), Ordering::Release);
    Ok(())
}

/// NX_DEVICE* flag bits that identify left/right modifiers in `CGEventFlags`.
fn modifier_bit(code: i64) -> Option<u64> {
    Some(match code {
        54 => 0x0010,    // right command
        55 => 0x0008,    // left command
        56 => 0x0002,    // left shift
        60 => 0x0004,    // right shift
        58 => 0x0020,    // left option
        61 => 0x0040,    // right option
        59 => 0x0001,    // left control
        62 => 0x2000,    // right control
        63 => 0x800000,  // fn
        57 => 0x10000,   // caps lock
        _ => return None,
    })
}

/// Virtual keycode → rdev-style name (so saved shortcuts stay portable across platforms).
pub fn key_name_from_code(code: i64) -> String {
    let name = match code {
        58 => "Alt",
        61 => "AltGr",
        51 => "Backspace",
        57 => "CapsLock",
        59 => "ControlLeft",
        62 => "ControlRight",
        125 => "DownArrow",
        53 => "Escape",
        122 => "F1",
        120 => "F2",
        99 => "F3",
        118 => "F4",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        100 => "F8",
        101 => "F9",
        109 => "F10",
        103 => "F11",
        111 => "F12",
        63 => "Function",
        123 => "LeftArrow",
        55 => "MetaLeft",
        54 => "MetaRight",
        36 => "Return",
        124 => "RightArrow",
        56 => "ShiftLeft",
        60 => "ShiftRight",
        49 => "Space",
        48 => "Tab",
        126 => "UpArrow",
        50 => "BackQuote",
        18 => "Num1",
        19 => "Num2",
        20 => "Num3",
        21 => "Num4",
        23 => "Num5",
        22 => "Num6",
        26 => "Num7",
        28 => "Num8",
        25 => "Num9",
        29 => "Num0",
        27 => "Minus",
        24 => "Equal",
        12 => "KeyQ",
        13 => "KeyW",
        14 => "KeyE",
        15 => "KeyR",
        17 => "KeyT",
        16 => "KeyY",
        32 => "KeyU",
        34 => "KeyI",
        31 => "KeyO",
        35 => "KeyP",
        33 => "LeftBracket",
        30 => "RightBracket",
        0 => "KeyA",
        1 => "KeyS",
        2 => "KeyD",
        3 => "KeyF",
        5 => "KeyG",
        4 => "KeyH",
        38 => "KeyJ",
        40 => "KeyK",
        37 => "KeyL",
        41 => "SemiColon",
        39 => "Quote",
        42 => "BackSlash",
        6 => "KeyZ",
        7 => "KeyX",
        8 => "KeyC",
        9 => "KeyV",
        11 => "KeyB",
        45 => "KeyN",
        46 => "KeyM",
        43 => "Comma",
        47 => "Dot",
        44 => "Slash",
        117 => "Delete",
        115 => "Home",
        119 => "End",
        116 => "PageUp",
        121 => "PageDown",
        76 => "KpReturn",
        _ => return format!("Unknown({code})"),
    };
    name.to_string()
}
