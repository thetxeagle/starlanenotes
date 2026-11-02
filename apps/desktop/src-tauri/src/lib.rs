#[cfg(desktop)]
mod desktop;

mod background_replay;
mod local_database;
mod native_notifications;
mod vault;

#[cfg(not(desktop))]
use std::sync::Mutex;
#[cfg(not(desktop))]
use tauri::{Emitter, Manager};
#[cfg(not(desktop))]
use tauri_plugin_deep_link::DeepLinkExt;

#[cfg(not(desktop))]
struct PendingNativeCallback(Mutex<Option<String>>);

#[cfg(not(desktop))]
#[tauri::command]
fn consume_pending_native_callback(app: tauri::AppHandle) -> Result<Option<String>, String> {
    app.state::<PendingNativeCallback>()
        .0
        .lock()
        .map_err(|error| error.to_string())
        .map(|mut pending| pending.take())
}

#[cfg(desktop)]
pub use desktop::run;

#[cfg(not(desktop))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_haptics::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            #[cfg(target_os = "linux")]
            app.manage(local_database::attachment_download_owner::native_archive_export::Selections::default());
            app.manage(local_database::attachment_download_owner::native_archive_capture::local_cover::transport::Producers::default());
            #[cfg(target_os = "linux")]
            app.manage(local_database::attachment_download_owner::native_archive_capture::local_import::transport::Producers::default());
            let pending = app
                .deep_link()
                .get_current()
                .ok()
                .flatten()
                .and_then(|urls| urls.last().map(ToString::to_string));
            app.manage(PendingNativeCallback(Mutex::new(pending)));

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let callback = url.to_string();
                    if let Ok(mut pending) = handle.state::<PendingNativeCallback>().0.lock() {
                        *pending = Some(callback.clone());
                    }
                    let _ = handle.emit("starlane-native-callback", callback);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_context::describe_current_native_vault_archive,
            local_database::attachment_download_owner::native_archive_context::list_original_native_trash_documents,
            local_database::attachment_download_owner::native_document_delete::delete_original_native_trash_document,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_context::read_imported_native_attachment_preview,
            local_database::attachment_download_owner::native_archive_context::cover::open_current_imported_native_cover_image,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_review::list_native_vault_archive_export_outcomes,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_review::acknowledge_native_vault_archive_export_review,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_export::choose_complete_native_vault_archive,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::prepare_original_native_local_import,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::prepare_original_native_local_import_path,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::run_original_native_local_import_path,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::inspect_original_native_local_import_path_source,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::recover_original_native_local_import_path_request,

            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::resume_original_native_local_import,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::append_original_native_local_import,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::finish_original_native_local_import,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_capture::local_import::transport::inspect_original_native_local_import,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::prepare_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::resume_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::discard_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::inspect_original_native_local_cover_discard,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::append_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::finish_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::transport::inspect_original_native_local_cover,
            local_database::attachment_download_owner::native_archive_capture::local_cover::image::open_original_native_local_cover_image,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_export::commit_complete_native_vault_archive,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::native_archive_export::discard_complete_native_vault_archive,
            consume_pending_native_callback,
            local_database::get_native_attachment_capabilities,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::capture_native_attachment_destination,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::prepare_native_attachment_download,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::append_native_attachment_download,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::finish_native_attachment_download,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::delivery::read_native_attachment_preview,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::image_stream::open_native_attachment_image,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::delivery::choose_native_attachment_export_destination,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::delivery::commit_native_attachment_export,
            #[cfg(target_os = "linux")]
            local_database::attachment_download_owner::delivery::discard_native_attachment_export_selection,
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
            vault::prepare_vault_archive_import,
            local_database::plan_native_vault_archive_import,
            local_database::commit_native_vault_archive_import,
            local_database::inspect_native_vault_archive_import,
            local_database::list_native_vault_archive_imports,
            local_database::abandon_native_vault_archive_import,
            vault::cancel_vault_archive_import,
            vault::import_vault_asset_from_path,
            vault::import_vault_asset_from_data_url,
            vault::delete_vault_asset,
            vault::pack_native_replay_asset_payload,
            background_replay::describe_native_background_replay_scheduler,
            background_replay::describe_native_background_replay_os_registration_plan,
            background_replay::register_native_background_replay_schedule,
            background_replay::read_native_background_replay_schedule,
            background_replay::clear_native_background_replay_schedule,
            background_replay::record_native_background_replay_wake,
            background_replay::import_native_background_replay_wake_receipt,
            local_database::describe_native_local_database,
            local_database::read_native_local_database_snapshot,
            local_database::write_native_local_database_snapshot,
            local_database::commit_native_local_database_snapshot,
            local_database::upsert_native_local_database_record,
            local_database::delete_native_local_database_record,
            local_database::sync_native_local_database_mirror,
            local_database::reset_native_local_database,
            native_notifications::send_native_notification,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Starlane Notes")
        .run(|_, _| {});
}
