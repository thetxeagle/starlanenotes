use serde_json::json;
use super::*;
use sqlx::sqlite::{SqliteConnectOptions,SqlitePoolOptions};
#[tokio::test(flavor="multi_thread",worker_threads=2)]
#[ignore="actual IPC refusal requires process-owned XDG directories"]
async fn actual_tauri_ipc_complete_archive_refusal_never_publishes_stale_mirror(){
    let work=PathBuf::from(std::env::var("STARLANE_NATIVE_ARCHIVE_REFUSAL_WORK").expect("owned fixture required"));assert!(work.is_absolute());
    let root=work.join("vault");fs::create_dir(&root).unwrap();fs::create_dir(root.join(".starlane")).unwrap();
    // A genuinely committed SQLite page differs from valid portable mirror JSON.
    let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")).create_if_missing(true)).await.unwrap();
    sqlx::query("CREATE TABLE local_vault_metadata(key TEXT PRIMARY KEY,value TEXT)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO local_vault_metadata VALUES('schema_version','1'),('canonical_revision','current-canonical'),('mirror_pending','current-canonical')").execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE local_documents(id TEXT PRIMARY KEY,space_id TEXT,title TEXT,payload TEXT)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO local_documents VALUES('current-page','space','Current page','{\"id\":\"current-page\",\"content\":\"actual canonical content\"}')").execute(&pool).await.unwrap();pool.close().await;
    let mirror=root.join("starlane-vault-none.json");fs::write(&mirror,b"{\"schema_version\":1,\"spaces\":[],\"folders\":[],\"documents\":[],\"assets\":[]}").unwrap();let before=fs::read(&mirror).unwrap();
    // Positive defect control: the preserved old physical exporter really
    // emits this valid stale mirror. The registered command must refuse it.
    let legacy=work.join("legacy-mirror-control.zip");archive_export_output::export(&root,&mirror,&legacy).unwrap();
    let mut zip=zip::ZipArchive::new(fs::File::open(&legacy).unwrap()).unwrap();let mut exported=Vec::new();zip.by_name(ARCHIVE_SNAPSHOT_FILE).unwrap().read_to_end(&mut exported).unwrap();assert_eq!(exported,before);
    let app=tauri::test::mock_builder().plugin(tauri_plugin_store::Builder::new().build()).invoke_handler(tauri::generate_handler![export_vault_archive]).build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
    let settings=tauri_plugin_store::resolve_store_path(app.handle(),"settings.json").unwrap();assert!(settings.starts_with(&work));fs::create_dir_all(settings.parent().unwrap()).unwrap();fs::write(&settings,serde_json::to_vec(&json!({"vault_path":root})).unwrap()).unwrap();
    let window=tauri::WebviewWindowBuilder::new(&app,"archive-refusal",Default::default()).build().unwrap();
    let new=work.join("new.starlane-vault");let prior=work.join("existing.starlane-vault");fs::write(&prior,b"previous complete archive").unwrap();
    for output in [&new,&prior]{
        let response=tauri::test::get_ipc_response(&window,tauri::webview::InvokeRequest{cmd:"export_vault_archive".into(),callback:tauri::ipc::CallbackFn(0),error:tauri::ipc::CallbackFn(1),url:"http://tauri.localhost".parse().unwrap(),body:tauri::ipc::InvokeBody::Json(json!({"provider":"none","archivePath":output})),headers:Default::default(),invoke_key:tauri::test::INVOKE_KEY.into()});
        let message=response.unwrap_err();assert!(message.as_str().unwrap().contains("Complete native vault export is unavailable"));
    }
    assert!(!new.exists());assert_eq!(fs::read(&prior).unwrap(),b"previous complete archive");assert_eq!(fs::read(&mirror).unwrap(),before);
    let files=fs::read_dir(&work).unwrap().map(|e|e.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();assert!(!files.iter().any(|n|n.starts_with(".starlane")));
    println!("ACTUAL_TAURI_IPC_STALE_MIRROR_NO_NEW_OUTPUT_NO_REPLACE_PASS");
}
