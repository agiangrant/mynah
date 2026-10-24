use crate::state::{AppState, TrayHandles};
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

pub const MAIN: &str = "main";

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open Mynah").build(app)?;
    let models = MenuItemBuilder::with_id("models", "Models…").build(app)?;
    let meeting = MenuItemBuilder::with_id("meeting", "Start meeting transcription").build(app)?;
    let model_item = MenuItemBuilder::with_id("model", "No model loaded").enabled(false).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Mynah").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &models])
        .item(&PredefinedMenuItem::separator(app)?)
        .items(&[&meeting, &model_item])
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&quit)
        .build()?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Mynah")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "models" => {
                show_main(app);
                let _ = app.emit("navigate", serde_json::json!({ "page": "models" }));
            }
            "meeting" => {
                let state = app.state::<AppState>();
                state.meetings.toggle();
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.meetings.stop();
                state.hub.shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    *app.state::<AppState>().tray.lock() = Some(TrayHandles { meeting_item: meeting, model_item });
    app.state::<AppState>().refresh_tray();
    Ok(())
}
