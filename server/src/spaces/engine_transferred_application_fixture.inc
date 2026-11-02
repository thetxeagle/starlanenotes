    #[cfg(target_os="linux")]
    #[tokio::test]
    #[ignore="explicit genuine transferred root, owned PG target and phase required"]
    async fn required_engine_transferred_application_child()->anyhow::Result<()> {
        use tower::ServiceExt;use anyhow::{ensure,Context};
        let _=tracing_subscriber::fmt().with_max_level(tracing::Level::ERROR).with_test_writer().try_init();
        let root=std::path::PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT").expect("root required"));
        let workspace=std::path::PathBuf::from(std::env::var("STARLANE_ENGINE_APP_WORKSPACE").expect("workspace required"));
        let mode=std::env::var("STARLANE_ENGINE_APP_MODE").expect("mode required");
        let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned target required");
        ensure!(root.is_absolute()&&workspace.is_absolute()&&root.starts_with(&workspace)&&matches!(mode.as_str(),"edit"|"edit-revoked"|"restart"),"Invalid engine app controls");
        ensure!(crate::storage::data_dir()==root,"DATA_DIR differs");
        ensure!(crate::storage::restoration::bootstrap_gate(&root).is_err(),"Plain bootstrap bypassed proof");
        let record:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("STARLANE_ENGINE_APP_RECORD").expect("producer record required"))?)?;
        let serving=crate::storage::restoration::open_restored_pg_serving(&root,&url).await?;
        crate::storage::init_data_dir().await?;
        crate::db::migrate_selected(serving.pool(),crate::db::DatabaseEngine::Postgres).await?;
        let state=crate::AppState{db:serving.pool().clone(),jwt_secret:"test-secret-with-enough-length-for-jwt".into(),base_url:"http://localhost:3000".into(),search_index:crate::search::SearchIndex::new(root.join("search_index"))?,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
        crate::storage::relocation::recover(&state.db).await.map_err(|e|anyhow::anyhow!("Actual relocation recovery: {e:?}"))?;
        crate::storage::journal::recover(&state.db).await.map_err(|e|anyhow::anyhow!("Actual journal recovery: {e:?}"))?;
        crate::storage::canonical_rewrite::recover(&state.db).await.map_err(|e|anyhow::anyhow!("Actual canonical_rewrite recovery: {e:?}"))?;
        crate::storage::publication::recover(&state.db).await.map_err(|e|anyhow::anyhow!("Actual publication recovery: {e:?}"))?;
        crate::storage::attachment_upload::recover_in(&root,&state.db).await.map_err(anyhow::Error::msg)?;
        crate::storage::restoration::fence(&root)?;serving.fence().await?;
        let old_id=record["operation_id"].as_str().context("original op missing")?;
        let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(old_id).fetch_one(&state.db).await?;
        ensure!(raw==record["raw_receipt"].as_str().unwrap(),"Original receipt rewritten");
        let expected=b"\0genuine original immutable upload\xff\r\nbytes";
        let app=crate::api_routes().with_state(state.clone());
        {let mut c=state.db.acquire().await?;super::check_attachment_page(&mut c,&auth("owner-a"),"space-a","target-a",crate::spaces::access::PageOperation::Edit).await.map_err(|e|anyhow::anyhow!("Actual owner page check: {e:?}"))?;}
        let retry=app.clone().oneshot(actual_upload_request(&state,old_id,expected,"producer.bin")).await?;
        let status=retry.status();let bytes=axum::body::to_bytes(retry.into_body(),10000).await?;
        ensure!(status==axum::http::StatusCode::CREATED,"Original retry failed: {status}: {}",String::from_utf8_lossy(&bytes));
        let response:serde_json::Value=serde_json::from_slice(&bytes)?;
        ensure!(response==record["response"],"Original response changed");
        let result_path=workspace.join("engine-app-result.json");
        if mode=="edit"||mode=="edit-revoked" {
            if mode=="edit-revoked"{let original=upload_page_json(&state,"GET","/spaces/space-a/documents/target-a/content",serde_json::Value::Null).await;ensure!(original.to_string().contains("PG_RESTORE_MUTABLE_BODY"),"Captured prior page body absent before second-generation edit");let uri=record["response"]["url"].as_str().unwrap().strip_prefix("/api/v1").unwrap();let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth("owner-a").claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()))?;let response=app.clone().oneshot(axum::http::Request::builder().uri(uri).header("authorization",format!("Bearer {token}")).body(axum::body::Body::empty())?).await?;ensure!(response.status()==axum::http::StatusCode::OK,"Captured original asset unavailable before edit");ensure!(axum::body::to_bytes(response.into_body(),10000).await?.as_ref()==expected,"Captured original asset bytes changed");}
            let draft=upload_page_json(&state,"POST","/spaces/space-a/documents/target-a/drafts",serde_json::json!({})).await;
            let id=draft["id"].as_str().or_else(||draft["draft_id"].as_str()).context("draft ID missing")?;
            let uri=format!("/spaces/space-a/documents/target-a/drafts/{id}");
            let selected=upload_page_json(&state,"GET",&uri,serde_json::Value::Null).await;
            upload_page_json(&state,"PUT",&uri,serde_json::json!({"source_digest":selected["source_digest"],"title":"PG restored editable page","content":"PG_RESTORE_MUTABLE_BODY Ω café"})).await;
            upload_page_json(&state,"POST",&format!("{uri}/publish"),serde_json::json!({})).await;
            let id=uuid::Uuid::new_v4().to_string();
            let response=app.clone().oneshot(actual_upload_request(&state,&id,b"\0new engine native upload\xff\r\n","engine-new.bin")).await?;
            ensure!(response.status()==axum::http::StatusCode::CREATED,"New upload failed: {}",response.status());
            let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),10000).await?)?;
            std::fs::write(&result_path,serde_json::to_vec(&serde_json::json!({"new_operation":id,"response":value}))?)?;
        }
        let result:serde_json::Value=serde_json::from_slice(&std::fs::read(&result_path)?)?;
        let page=upload_page_json(&state,"GET","/spaces/space-a/documents/target-a/content",serde_json::Value::Null).await;
        ensure!(page.to_string().contains("PG_RESTORE_MUTABLE_BODY"),"Edited body missing after admission");
        for (value,bytes) in [(&record["response"],expected.as_slice()),(&result["response"],b"\0new engine native upload\xff\r\n".as_slice())] {
            let uri=value["url"].as_str().context("asset URL missing")?.strip_prefix("/api/v1").context("API prefix missing")?;
            for actor in [Some("owner-a"),None] {
                let mut q=axum::http::Request::builder().uri(uri);
                if let Some(actor)=actor {let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth(actor).claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()))?;q=q.header("authorization",format!("Bearer {token}"));}
                let response=app.clone().oneshot(q.body(axum::body::Body::empty())?).await?;
                if actor.is_some(){ensure!(response.status()==axum::http::StatusCode::OK,"Private download failed: {}",response.status());ensure!(&axum::body::to_bytes(response.into_body(),10000).await?[..]==bytes,"Exact binary bytes differ");}
                else{ensure!(matches!(response.status(),axum::http::StatusCode::UNAUTHORIZED|axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND),"Anonymous asset did not return actual auth policy denial: {}",response.status());let body=axum::body::to_bytes(response.into_body(),10000).await?;let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(!v.to_string().contains("genuine original immutable upload")&&["id","storage_path","filename","mime_type","size_bytes","receipt_json"].iter().all(|k|v.get(k).is_none()),"Anonymous denial exposed asset metadata");}
            }
        }
        ensure!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(old_id).fetch_one(&state.db).await?==raw,"Old receipt changed after mutation");
        // Actual authenticated viewer and post-revocation outsider state, not
        // merely a denied token for a nonexistent fixture identity.
        let viewer_token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth("viewer-a").claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()))?;
        let owner_token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth("owner-a").claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()))?;
        for token in [&owner_token,&viewer_token] {
          for search_uri in ["/search?title=PG%20restored%20editable%20page","/search?q=PG_RESTORE_MUTABLE_BODY"] {
            let response=app.clone().oneshot(axum::http::Request::builder().uri(search_uri).header("authorization",format!("Bearer {token}")).body(axum::body::Body::empty())?).await?;
            ensure!(response.status()==axum::http::StatusCode::OK,"Search route failed: {}",response.status());
            let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),100000).await?)?;
            let found=value.as_array().context("Actual search result shape")?.iter().any(|v|v["document_id"]=="target-a");
            ensure!(found==(token==&owner_token||mode=="edit"),"Current restored search audience differs ({search_uri}): {value}");
          }
        }
        if mode=="edit" {
            for uri in ["/spaces/space-a/documents/target-a/content",record["response"]["url"].as_str().unwrap().strip_prefix("/api/v1").unwrap()] {
                let response=app.clone().oneshot(axum::http::Request::builder().uri(uri).header("authorization",format!("Bearer {viewer_token}")).body(axum::body::Body::empty())?).await?;
                ensure!(response.status()==axum::http::StatusCode::OK,"Restored viewer read failed: {}",response.status());let body=axum::body::to_bytes(response.into_body(),100000).await?;if uri.ends_with("/content"){let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(v.to_string().contains("PG_RESTORE_MUTABLE_BODY")&&v.to_string().contains("PG restored editable page"),"Viewer received wrong page bytes/metadata");}else{ensure!(body.as_ref()==expected,"Viewer received wrong exact asset bytes");}
            }
            let response=app.clone().oneshot(axum::http::Request::builder().method("POST").uri("/spaces/space-a/documents/target-a/drafts").header("authorization",format!("Bearer {viewer_token}")).header("content-type","application/json").body(axum::body::Body::from("{}"))?).await?;
            ensure!(matches!(response.status(),axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND),"Viewer write did not return a policy denial: {}",response.status());let body=axum::body::to_bytes(response.into_body(),10000).await?;let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(!v.to_string().contains("PG_RESTORE_MUTABLE_BODY")&&v.get("id").is_none()&&v.get("content").is_none(),"Viewer write denial disclosed protected state");
            let response=app.clone().oneshot(axum::http::Request::builder().method("DELETE").uri("/spaces/space-a/members/viewer-a").header("authorization",format!("Bearer {owner_token}")).body(axum::body::Body::empty())?).await?;
            ensure!(response.status()==axum::http::StatusCode::NO_CONTENT,"Actual membership revoke failed: {}",response.status());ensure!(axum::body::to_bytes(response.into_body(),10000).await?.is_empty(),"Revoke returned unexpected metadata");
        }
        for uri in ["/spaces/space-a/documents/target-a/content",record["response"]["url"].as_str().unwrap().strip_prefix("/api/v1").unwrap()] {
            let response=app.clone().oneshot(axum::http::Request::builder().uri(uri).header("authorization",format!("Bearer {viewer_token}")).body(axum::body::Body::empty())?).await?;
            ensure!(matches!(response.status(),axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND),"Revoked actor did not return a policy denial: {}",response.status());let body=axum::body::to_bytes(response.into_body(),10000).await?;let v:serde_json::Value=serde_json::from_slice(&body)?;let text=v.to_string();ensure!(!text.contains("PG_RESTORE_MUTABLE_BODY")&&!text.contains("PG restored editable page")&&!text.contains("genuine original immutable upload")&&["id","document_id","storage_path","title","content","filename","mime_type","size_bytes","created_by","receipt_json"].iter().all(|k|v.get(k).is_none()),"Revoked denial disclosed protected page/asset state");
        }
        let response=app.clone().oneshot(axum::http::Request::builder().uri("/search?title=PG%20restored%20editable%20page").header("authorization",format!("Bearer {viewer_token}")).body(axum::body::Body::empty())?).await?;
        ensure!(response.status()==axum::http::StatusCode::OK,"Revoked global search route failed");let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),100000).await?)?;ensure!(value.as_array().context("Revoked search result shape")?.iter().all(|v|v["document_id"]!="target-a"),"Revoked search disclosed target page");
        serving.fence().await?;crate::storage::restoration::fence(&root)?;
        state.db.close().await;
        println!("ENGINE_APP_{mode}_PASS actual mutable draft publish/new upload/private binary read/exact old retry");
        Ok(())
    }

    #[cfg(target_os="linux")]
    #[tokio::test]
    #[ignore="actual restored PG root, live sessions and explicit audience phase required"]
    async fn required_engine_restored_audiences_child()->anyhow::Result<()> {
        use anyhow::{ensure,Context}; use tower::ServiceExt; use axum::http::StatusCode;
        let root=std::path::PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);
        let workspace=std::path::PathBuf::from(std::env::var("STARLANE_ENGINE_APP_WORKSPACE")?);
        let mode=std::env::var("STARLANE_ENGINE_AUDIENCE_MODE")?;
        ensure!(root.is_absolute()&&workspace.is_absolute()&&root.starts_with(&workspace)&&crate::storage::data_dir()==root&&matches!(mode.as_str(),"seed"|"read"|"revoke"|"restart"),"Invalid audience controls");
        let serving=crate::storage::restoration::open_restored_pg_serving(&root,&std::env::var("STARLANE_ENGINE_PG_URL")?).await?;
        crate::storage::init_data_dir().await?;
        let state=crate::AppState{db:serving.pool().clone(),jwt_secret:"test-secret-with-enough-length-for-jwt".into(),base_url:"http://localhost:3000".into(),search_index:crate::search::SearchIndex::new(root.join("search_index"))?,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
        let app=crate::api_routes().with_state(state.clone());
        async fn request(app:&axum::Router,state:&crate::AppState,actor:Option<&str>,method:&str,uri:&str,value:serde_json::Value)->anyhow::Result<(StatusCode,Vec<u8>)>{
            let mut q=axum::http::Request::builder().method(method).uri(uri).header("content-type","application/json");
            if let Some(actor)=actor {let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth(actor).claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()))?;q=q.header("authorization",format!("Bearer {token}"));}
            let response=app.clone().oneshot(q.body(axum::body::Body::from(value.to_string()))?).await?;let status=response.status();let body=axum::body::to_bytes(response.into_body(),100000).await?;Ok((status,body.to_vec()))
        }
        fn denied(status:StatusCode,body:&[u8])->anyhow::Result<()> {
            ensure!(matches!(status,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND),"Expected current authenticated policy denial, got {status}: {}",String::from_utf8_lossy(body));
            let v:serde_json::Value=serde_json::from_slice(body)?;let text=v.to_string();ensure!(!text.contains("PG_RESTORE_MUTABLE_BODY")&&!text.contains("PG restored editable page")&&!text.contains("genuine original immutable upload")&&["id","document_id","content","title","storage_path","filename","mime_type","size_bytes","receipt_json"].iter().all(|k|v.get(k).is_none()),"Denial exposed protected payload");Ok(())
        }
        let record:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("STARLANE_ENGINE_APP_RECORD")?)?)?;
        let asset=record["response"]["url"].as_str().context("Original actual asset URL")?.strip_prefix("/api/v1").context("API prefix")?;
        let original_receipt:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(record["operation_id"].as_str().unwrap()).fetch_one(&state.db).await?;
        let receipt_path=workspace.join("audience-record.json");
        if mode=="seed" {
            // The selected repository has no grant-management writer/API.
            // Fixture seeds genuine application principals/sessions/grant rows only;
            // no custody, raw operation proof or restore authority is authored here.
            for actor in ["direct-a","guest-a","outsider-a","audience-admin"] {
                sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,$3)").bind(actor).bind(format!("{actor}@example.test")).bind(if actor=="audience-admin"{"admin"}else{"member"}).execute(&state.db).await?;
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'audience-fixture','2999-01-01T00:00:00Z')").bind(format!("asset-{actor}")).bind(actor).execute(&state.db).await?;
            }
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('guest-legacy-membership','space-a','guest-a','editor')").execute(&state.db).await?;
            for actor in ["direct-a","guest-a"] {sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,'target-a','space-a',$2,'viewer','page','audience-original','owner-a','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z')").bind(format!("audience-{actor}")).bind(actor).execute(&state.db).await?;}
            let(status,body)=request(&app,&state,Some("audience-admin"),"POST","/admin/users/guest-a/guest/review",serde_json::json!({})).await?;ensure!(status==StatusCode::OK,"Actual guest review: {status}: {}",String::from_utf8_lossy(&body));let review:serde_json::Value=serde_json::from_slice(&body)?;
            let(status,body)=request(&app,&state,Some("audience-admin"),"POST","/admin/users/guest-a/guest/confirm",serde_json::json!({"source_digest":review["source_digest"],"confirm":true})).await?;ensure!(status==StatusCode::OK&&serde_json::from_slice::<serde_json::Value>(&body)?["account_kind"]=="guest","Actual guest classification failed");
            let(status,body)=request(&app,&state,Some("owner-a"),"POST","/spaces/space-a/documents/target-a/publish",serde_json::json!({})).await?;ensure!(status==StatusCode::OK,"Actual public-link enable {status}: {}",String::from_utf8_lossy(&body));let value:serde_json::Value=serde_json::from_slice(&body)?;ensure!(value["public_slug"].as_str().is_some(),"Actual public slug absent");std::fs::write(&receipt_path,serde_json::to_vec(&value)?)?;
        }
        let public:serde_json::Value=serde_json::from_slice(&std::fs::read(&receipt_path)?)?;let public_uri=format!("/p/{}",public["public_slug"].as_str().context("Retained genuine slug")?);
        let revoked=mode=="revoke"||mode=="restart";
        if mode=="revoke" {
            // No production direct-grant writer exists; explicit application-row
            // revocation exercises current readers, not management availability.
            ensure!(sqlx::query("DELETE FROM document_viewer_grants WHERE id IN ('audience-direct-a','audience-guest-a')").execute(&state.db).await?.rows_affected()==2,"Exact grant revoke count differs");
            let(status,body)=request(&app,&state,Some("owner-a"),"POST","/spaces/space-a/documents/target-a/unpublish",serde_json::json!({})).await?;ensure!(status==StatusCode::NO_CONTENT&&body.is_empty(),"Actual public revoke did not acknowledge empty204");
        }
        // Rebuild excluded derived search using the REAL selected admin route;
        // no test-authored Tantivy entries or body edits manufacture visibility.
        let(status,body)=request(&app,&state,Some("audience-admin"),"POST","/admin/search/reindex",serde_json::json!({})).await?;ensure!(status==StatusCode::OK&&serde_json::from_slice::<serde_json::Value>(&body)?["status"]=="reindex_complete","Actual restored source reindex failed: {status}");
        for actor in ["direct-a","guest-a","outsider-a"] {
            let allowed=actor!="outsider-a"&&!revoked;
            for uri in ["/spaces/space-a/documents/target-a/content",asset] {
                let(status,body)=request(&app,&state,Some(actor),"GET",uri,serde_json::Value::Null).await?;
                if allowed {ensure!(status==StatusCode::OK,"Granted {actor} route failed {uri}: {status}");if uri==asset {ensure!(body==b"\0genuine original immutable upload\xff\r\nbytes","Granted asset byte mismatch");}else{let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(v.to_string().contains("PG_RESTORE_MUTABLE_BODY")&&v.to_string().contains("PG restored editable page"),"Granted page body/metadata differs");}}
                else{denied(status,&body)?;}
            }
            let(status,body)=request(&app,&state,Some(actor),"POST","/spaces/space-a/documents/target-a/drafts",serde_json::json!({})).await?;denied(status,&body)?;
            for uri in ["/search?title=PG%20restored%20editable%20page","/search?q=PG_RESTORE_MUTABLE_BODY"] {let(status,body)=request(&app,&state,Some(actor),"GET",uri,serde_json::Value::Null).await?;ensure!(status==StatusCode::OK,"Current {actor} search status {status}");let value:serde_json::Value=serde_json::from_slice(&body)?;ensure!(value.as_array().context("Search shape")?.iter().any(|v|v["document_id"]=="target-a")==allowed,"Current audience search differs");}
        }
        let(status,body)=request(&app,&state,None,"GET",&public_uri,serde_json::Value::Null).await?;
        if revoked {ensure!(status==StatusCode::NOT_FOUND,"Revoked anonymous public page status {status}");denied(status,&body)?;}else{ensure!(status==StatusCode::OK,"Live public page status {status}");let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(v["title"]=="PG restored editable page"&&v["content"].as_str().is_some_and(|s|s.contains("PG_RESTORE_MUTABLE_BODY")),"Actual public page bytes differ");}
        let(status,body)=request(&app,&state,None,"GET",asset,serde_json::Value::Null).await?;ensure!(status==StatusCode::UNAUTHORIZED,"Public page incorrectly grants private asset URL: {status}");let v:serde_json::Value=serde_json::from_slice(&body)?;ensure!(["id","storage_path","content","filename"].iter().all(|k|v.get(k).is_none())&&!String::from_utf8_lossy(&body).contains("genuine original immutable upload"),"Anonymous private asset leaked");
        ensure!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(record["operation_id"].as_str().unwrap()).fetch_one(&state.db).await?==original_receipt,"Audience work rewrote original upload receipt");
        serving.fence().await?;crate::storage::restoration::fence(&root)?;state.db.close().await;
        println!("ENGINE_AUDIENCES_{mode}_PASS exact direct/guest/outsider/public policy + bytes/search/current revoke; no grant-management producer claim");Ok(())
    }
