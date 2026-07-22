use crate::vault;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::UpdaterExt;

/// Counter for unique secondary window labels
static WINDOW_COUNTER: AtomicU32 = AtomicU32::new(2);

// ============================================================
// State Types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowState {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateInfo {
    version: String,
    body: Option<String>,
}

// ============================================================
// IPC Commands — Server URL & Window State
// ============================================================

#[tauri::command]
fn get_server_url(app: tauri::AppHandle) -> Result<String, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    match store.get("server_url") {
        Some(val) => val
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "server_url is not a string".to_string()),
        None => Ok(String::new()),
    }
}

#[tauri::command]
fn set_server_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set("server_url", serde_json::json!(url));
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_window_state(app: tauri::AppHandle) -> Result<Option<WindowState>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    match store.get("window_state") {
        Some(val) => {
            let state: WindowState =
                serde_json::from_value(val.clone()).map_err(|e| e.to_string())?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}

#[tauri::command]
fn save_window_state(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let state = WindowState {
        x,
        y,
        width,
        height,
    };
    store.set("window_state", serde_json::to_value(&state).unwrap());
    store.save().map_err(|e| e.to_string())
}

// ============================================================
// IPC Commands — Multiple Windows
// ============================================================

#[tauri::command]
fn open_new_window(app: tauri::AppHandle) -> Result<(), String> {
    let count = WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("window-{count}");

    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("/".into()))
        .title("Starlane Notes")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .build()
        .map_err(|e: tauri::Error| e.to_string())?;

    Ok(())
}

// ============================================================
// IPC Commands — Auto-Updater
// ============================================================

/// Check for an available update. Returns None if up to date.
#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfo {
            version: update.version.to_string(),
            body: update.body.clone(),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Download and install the available update, emitting progress events.
/// Restarts the app on completion.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;

    let Some(update) = update else {
        return Err("No update available".to_string());
    };

    let handle = app.clone();
    update
        .download_and_install(
            move |downloaded, total| {
                let percent = total
                    .map(|t| (downloaded as f64 / t as f64 * 100.0) as u32)
                    .unwrap_or(0);
                let _ = handle.emit("update-progress", percent);
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;

    app.restart();
}

// ============================================================
// Background Update Checker
// ============================================================

fn spawn_update_checker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Wait for the frontend to finish loading
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        loop {
            if let Ok(updater) = app.updater() {
                match updater.check().await {
                    Ok(Some(update)) => {
                        let info = UpdateInfo {
                            version: update.version.to_string(),
                            body: update.body.clone(),
                        };
                        let _ = app.emit("update-available", &info);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        // Update check failed (network unavailable or no pubkey configured)
                    }
                }
            }

            // Re-check every 4 hours
            tokio::time::sleep(std::time::Duration::from_secs(4 * 60 * 60)).await;
        }
    });
}

// ============================================================
// Menu Construction
// ============================================================

fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;

    // Starlane Notes menu
    let app_menu = Submenu::with_items(
        app,
        "Starlane Notes",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Starlane Notes"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "preferences",
                "Preferences…",
                true,
                Some("CmdOrCtrl+,"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // File menu
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "new_note", "New Note", true, Some("CmdOrCtrl+N"))?,
            &MenuItem::with_id(
                app,
                "new_space",
                "New Space",
                true,
                Some("CmdOrCtrl+Shift+N"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "export_document",
                "Export Document…",
                true,
                Some("CmdOrCtrl+E"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    // Edit menu
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "find", "Find…", true, Some("CmdOrCtrl+F"))?,
        ],
    )?;

    // View menu
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(
                app,
                "toggle_sidebar",
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+\\"),
            )?,
            &MenuItem::with_id(
                app,
                "toggle_dark_mode",
                "Toggle Dark Mode",
                true,
                Some("CmdOrCtrl+Shift+D"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "zoom_in", "Zoom In", true, Some("CmdOrCtrl+Plus"))?,
            &MenuItem::with_id(app, "zoom_out", "Zoom Out", true, Some("CmdOrCtrl+-"))?,
            &MenuItem::with_id(app, "zoom_reset", "Reset Zoom", true, Some("CmdOrCtrl+0"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    // Go menu
    let go_menu = Submenu::with_items(
        app,
        "Go",
        true,
        &[
            &MenuItem::with_id(app, "search", "Search…", true, Some("CmdOrCtrl+K"))?,
            &MenuItem::with_id(app, "go_back", "Back", true, Some("CmdOrCtrl+["))?,
            &MenuItem::with_id(app, "go_forward", "Forward", true, Some("CmdOrCtrl+]"))?,
        ],
    )?;

    // Window menu
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "new_window",
                "New Window",
                true,
                Some("CmdOrCtrl+Alt+N"),
            )?,
        ],
    )?;

    // Help menu
    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[&MenuItem::with_id(
            app,
            "keyboard_shortcuts",
            "Keyboard Shortcuts",
            true,
            Some("CmdOrCtrl+?"),
        )?],
    )?;

    menu.append(&app_menu)?;
    menu.append(&file_menu)?;
    menu.append(&edit_menu)?;
    menu.append(&view_menu)?;
    menu.append(&go_menu)?;
    menu.append(&window_menu)?;
    menu.append(&help_menu)?;

    Ok(menu)
}

fn handle_menu_event(app: &tauri::AppHandle, event: MenuEvent) {
    let id = event.id().0.as_str();
    match id {
        "new_window" => {
            let count = WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst);
            let label = format!("window-{count}");
            let _ =
                tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::App("/".into()))
                    .title("Starlane Notes")
                    .inner_size(1280.0, 800.0)
                    .min_inner_size(900.0, 600.0)
                    .build();
        }
        _ => {
            let _ = app.emit("menu-action", id);
        }
    }
}

// ============================================================
// Window State Persistence
// ============================================================

fn persist_window_state(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let position = window.outer_position().ok();
        let size = window.outer_size().ok();

        if let (Some(pos), Some(sz)) = (position, size) {
            if let Ok(store) = app.store("settings.json") {
                let state = WindowState {
                    x: pos.x as f64,
                    y: pos.y as f64,
                    width: sz.width as f64,
                    height: sz.height as f64,
                };
                store.set("window_state", serde_json::to_value(&state).unwrap());
                let _ = store.save();
            }
        }
    }
}

fn restore_window_state(app: &tauri::AppHandle) {
    if let Ok(store) = app.store("settings.json") {
        if let Some(val) = store.get("window_state") {
            if let Ok(state) = serde_json::from_value::<WindowState>(val.clone()) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition::new(state.x as i32, state.y as i32),
                    ));
                    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
                        state.width as u32,
                        state.height as u32,
                    )));
                }
            }
        }
    }
}

// ============================================================
// System Tray
// ============================================================

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let tray_menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "tray_open", "Open", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "tray_sync_status",
                "Sync: Connected",
                false,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "tray_quick_note", "Quick Note", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "tray_preferences", "Preferences…", true, None::<&str>)?,
            &MenuItem::with_id(app, "tray_quit", "Quit Starlane Notes", true, None::<&str>)?,
        ],
    )?;

    TrayIconBuilder::new()
        .icon_as_template(true)
        .icon(
            app.default_window_icon()
                .cloned()
                .or_else(|| {
                    tauri::image::Image::from_path(
                        app.path().resource_dir().ok()?.join("icons/icon.png"),
                    )
                    .ok()
                })
                .expect("no tray icon available — add icons/icon.png or configure bundle icons"),
        )
        .menu(&tray_menu)
        .on_menu_event(|app, event| {
            let id = event.id().0.as_str();
            match id {
                "tray_open" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "tray_quit" => {
                    app.exit(0);
                }
                "tray_preferences" | "tray_quick_note" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit("menu-action", id.strip_prefix("tray_").unwrap_or(id));
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
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

// ============================================================
// Deep Link State
// ============================================================

struct PendingDeepLink(Mutex<Option<String>>);

#[tauri::command]
fn consume_pending_deep_link(app: tauri::AppHandle) -> Result<Option<DeepLinkRoute>, String> {
    let state = app.state::<PendingDeepLink>();
    let url = state.0.lock().map_err(|e| e.to_string())?.take();
    match url {
        Some(u) => Ok(parse_deep_link(&u)),
        None => Ok(None),
    }
}

// ============================================================
// Utility
// ============================================================

fn percent_decode(input: &str) -> String {
    let mut result = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
            {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(b' ');
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

// ============================================================
// Deep Link Handling
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeepLinkRoute {
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    space_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
}

fn parse_deep_link(url: &str) -> Option<DeepLinkRoute> {
    let path = url
        .strip_prefix("starlane://")
        .or_else(|| url.strip_prefix("starlane:"))?;

    let (path_part, query_string) = match path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path, None),
    };

    let segments: Vec<&str> = path_part.trim_matches('/').split('/').collect();

    match segments.first().copied() {
        Some("open") => Some(DeepLinkRoute {
            action: "open".to_string(),
            space_id: None,
            doc_id: None,
            query: None,
        }),
        Some("note") if segments.len() >= 3 => Some(DeepLinkRoute {
            action: "open_note".to_string(),
            space_id: Some(segments[1].to_string()),
            doc_id: Some(segments[2].to_string()),
            query: None,
        }),
        Some("space") if segments.len() >= 2 => Some(DeepLinkRoute {
            action: "open_space".to_string(),
            space_id: Some(segments[1].to_string()),
            doc_id: None,
            query: None,
        }),
        Some("new") => Some(DeepLinkRoute {
            action: "new_document".to_string(),
            space_id: None,
            doc_id: None,
            query: None,
        }),
        Some("search") => {
            let query = query_string.and_then(|qs| {
                qs.split('&').find_map(|param| {
                    let (key, value) = param.split_once('=')?;
                    if key == "q" {
                        Some(percent_decode(value))
                    } else {
                        None
                    }
                })
            });
            Some(DeepLinkRoute {
                action: "search".to_string(),
                space_id: None,
                doc_id: None,
                query,
            })
        }
        _ => None,
    }
}

fn handle_deep_link_url(app: &tauri::AppHandle, url: &str) {
    if let Some(route) = parse_deep_link(url) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app.emit("deep-link", &route);
    }
}

// ============================================================
// Main
// ============================================================

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_haptics::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_server_url,
            set_server_url,
            get_window_state,
            save_window_state,
            consume_pending_deep_link,
            open_new_window,
            crate::native_notifications::send_native_notification,
            check_for_update,
            install_update,
            vault::get_vault_location,
            vault::get_vault_location_status,
            vault::list_vault_folders,
            vault::create_vault_folder,
            vault::rename_vault_folder,
            vault::rename_vault_folder_at,
            vault::delete_vault_folder_at,
            vault::move_vault_to_icloud,
            vault::write_icloud_provider_sync_manifest,
            vault::set_vault_path,
            vault::clear_vault_path,
            vault::clear_vault_path_without_initializing,
            vault::reveal_vault_folder,
            vault::read_vault_snapshot,
            vault::write_vault_snapshot,
            vault::reconcile_vault_mirror,
            vault::reset_vault_snapshot,
            vault::export_vault_archive,
            vault::import_vault_archive,
            vault::import_vault_asset_from_path,
            vault::import_vault_asset_from_data_url,
            vault::pack_native_replay_asset_payload,
            crate::background_replay::describe_native_background_replay_scheduler,
            crate::background_replay::describe_native_background_replay_os_registration_plan,
            crate::background_replay::register_native_background_replay_schedule,
            crate::background_replay::read_native_background_replay_schedule,
            crate::background_replay::clear_native_background_replay_schedule,
            crate::background_replay::record_native_background_replay_wake,
            crate::background_replay::import_native_background_replay_wake_receipt,
            crate::local_database::describe_native_local_database,
            crate::local_database::read_native_local_database_snapshot,
            crate::local_database::write_native_local_database_snapshot,
            crate::local_database::upsert_native_local_database_record,
            crate::local_database::delete_native_local_database_record,
            crate::local_database::reset_native_local_database,
        ])
        .setup(|app| {
            let handle = app.handle();

            let menu = build_app_menu(handle)?;
            app.set_menu(menu)?;
            app.on_menu_event(move |app, event| handle_menu_event(app, event));

            build_tray(handle)?;
            restore_window_state(handle);

            // Deep link: store pending URL for when frontend is ready
            let pending_deep_link: Mutex<Option<String>> = Mutex::new(None);

            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                if let Some(url) = urls.first() {
                    let url_str = url.to_string();
                    *pending_deep_link.lock().unwrap() = Some(url_str);
                }
            }

            app.manage(PendingDeepLink(pending_deep_link));

            let deep_link_handle = handle.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    handle_deep_link_url(&deep_link_handle, &url.to_string());
                }
            });

            // Spawn background update checker (checks on startup + every 4h)
            spawn_update_checker(handle.clone());

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Starlane Notes")
        .run(|app, event| match event {
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } if label == "main" => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                persist_window_state(app);
            }
            RunEvent::ExitRequested { api, .. } => {
                let _ = api;
            }
            _ => {}
        });
}
