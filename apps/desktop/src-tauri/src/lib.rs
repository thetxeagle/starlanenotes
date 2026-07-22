#[cfg(desktop)]
mod desktop;

mod background_replay;
mod local_database;
mod native_notifications;
mod vault;

#[cfg(desktop)]
pub use desktop::run;

#[cfg(not(desktop))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_haptics::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
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
            local_database::upsert_native_local_database_record,
            local_database::delete_native_local_database_record,
            local_database::reset_native_local_database,
            native_notifications::send_native_notification,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Starlane Notes")
        .run(|_, _| {});
}
