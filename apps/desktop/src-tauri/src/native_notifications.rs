use tauri_plugin_notification::NotificationExt;

#[tauri::command]
pub fn send_native_notification(
    app: tauri::AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
        .map_err(|error| error.to_string())
}
