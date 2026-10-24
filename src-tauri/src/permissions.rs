//! OS permission checks. Only macOS gates these behind TCC; elsewhere we report `null`.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Permissions {
    pub accessibility: Option<bool>,
    pub input_monitoring: Option<bool>,
    pub microphone: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Accessibility,
    InputMonitoring,
    Microphone,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::{CFString, CFStringRef};
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    fn microphone_status() -> Option<bool> {
        // AVAuthorizationStatus: 0 notDetermined, 1 restricted, 2 denied, 3 authorized
        let cls = AnyClass::get(c"AVCaptureDevice")?;
        let media = NSString::from_str("soun");
        let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] };
        match status {
            3 => Some(true),
            0 => None,
            _ => Some(false),
        }
    }

    pub fn check() -> Permissions {
        Permissions {
            accessibility: Some(unsafe { AXIsProcessTrusted() }),
            input_monitoring: Some(unsafe { CGPreflightListenEventAccess() }),
            microphone: Some(microphone_status().unwrap_or(false)),
        }
    }

    pub fn request(kind: PermissionKind) {
        match kind {
            PermissionKind::Accessibility => unsafe {
                let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
                let dict = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
                AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef());
            },
            PermissionKind::InputMonitoring => unsafe {
                CGRequestListenEventAccess();
            },
            PermissionKind::Microphone => {
                // Opening an input stream triggers the system prompt.
                std::thread::spawn(|| {
                    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
                    let host = cpal::default_host();
                    if let Some(dev) = host.default_input_device() {
                        if let Ok(cfg) = dev.default_input_config() {
                            if let Ok(stream) = dev.build_input_stream(cfg.config(), |_: &[f32], _| {}, |_| {}, None) {
                                let _ = stream.play();
                                std::thread::sleep(std::time::Duration::from_millis(800));
                            }
                        }
                    }
                });
            }
        }
    }

    pub fn open_settings(kind: PermissionKind) {
        let pane = match kind {
            PermissionKind::Accessibility => "Privacy_Accessibility",
            PermissionKind::InputMonitoring => "Privacy_ListenEvent",
            PermissionKind::Microphone => "Privacy_Microphone",
        };
        let _ = std::process::Command::new("open")
            .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
            .spawn();
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    pub fn check() -> Permissions {
        Permissions::default()
    }
    pub fn request(_kind: PermissionKind) {}
    pub fn open_settings(_kind: PermissionKind) {}
}

pub use imp::{check, open_settings, request};
