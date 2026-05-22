use std::sync::atomic::Ordering;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    App, Emitter, Manager,
};

pub fn setup_tray(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    // Read initial debug-server state from AppState so the check mark
    // reflects the running listener.
    let debug_server_initial = app
        .state::<crate::AppState>()
        .debug_server_running
        .load(Ordering::SeqCst);

    // Create menu items
    let show = MenuItem::with_id(app, "show", "Show Ondo", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let debug_server = CheckMenuItem::with_id(
        app,
        "debug_server",
        "Debug server (port 19210)",
        true,
        debug_server_initial,
        None::<&str>,
    )?;
    let separator = MenuItem::with_id(app, "sep", "─────────", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    // Build menu
    let menu = Menu::with_items(
        app,
        &[&show, &settings, &debug_server, &separator, &quit],
    )?;

    // The check item handle is captured by the menu-event closure so we can
    // sync its state if the toggle succeeds.
    let debug_server_check = debug_server.clone();

    // Create tray icon
    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Ondo - Hardware Monitor")
        .on_menu_event(move |app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "settings" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        // Emit event to open settings
                        let _ = window.emit("open-settings", ());
                    }
                }
                "debug_server" => {
                    let state = app.state::<crate::AppState>();
                    let was_running = state.debug_server_running.load(Ordering::SeqCst);
                    let new_state = !was_running;

                    if let Err(e) = state.set_debug_server(new_state) {
                        crate::log_error!(
                            "Tray",
                            "Failed to toggle debug server to {}: {}",
                            new_state,
                            e
                        );
                        // Revert the check mark to actual state.
                        let actual = state.debug_server_running.load(Ordering::SeqCst);
                        let _ = debug_server_check.set_checked(actual);
                        return;
                    }

                    // Reflect on the check item.
                    let _ = debug_server_check.set_checked(new_state);

                    // Persist the new value so it survives restart.
                    let settings_to_save = match state.settings.lock() {
                        Ok(mut guard) => {
                            guard.debug_server = new_state;
                            guard.clone()
                        }
                        Err(e) => {
                            crate::log_error!(
                                "Tray",
                                "settings lock poisoned while persisting debug_server: {}",
                                e
                            );
                            return;
                        }
                    };
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            crate::settings::save_settings_to_file(&settings_to_save).await
                        {
                            crate::log_error!(
                                "Tray",
                                "Failed to persist debug_server setting: {}",
                                e
                            );
                        }
                    });

                    crate::log_info!(
                        "Tray",
                        "Debug server toggled to {} from tray",
                        new_state
                    );
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
