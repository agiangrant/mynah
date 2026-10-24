//! The floating status pill window.

use tauri::{AppHandle, Manager, PhysicalPosition};

pub const LABEL: &str = "overlay";
const BOTTOM_MARGIN_LOGICAL: f64 = 72.0;

pub fn show(app: &AppHandle) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    // Position at the bottom centre of the monitor under the cursor.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let (Some(mon), Ok(size)) = (monitor, win.outer_size()) {
        let area = mon.work_area();
        let scale = mon.scale_factor();
        let x = area.position.x + ((area.size.width as i32 - size.width as i32) / 2).max(0);
        let y = area.position.y + area.size.height as i32 - size.height as i32 - (BOTTOM_MARGIN_LOGICAL * scale) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    if !win.is_visible().unwrap_or(false) {
        let _ = win.show();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.hide();
    }
}
