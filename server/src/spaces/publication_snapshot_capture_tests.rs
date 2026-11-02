//! Include as a sibling #[cfg(test)] module inside publication_snapshots.
use super::*;
fn capture()->Capture {
    let comparisons=Comparisons{properties_schema:vec![],property_values:vec![],attachments:vec![],drawings:vec![]};
    let result=payload(Binding::CaptureV2,"target-a","space-a",content("New".into(),None,None,Some("RAW".into()),Extraction::ExactPublishedDraft,b"RAW").unwrap(),comparisons);
    Capture{result,result_raw:b"RAW".to_vec(),baseline:None,baseline_raw:None,baseline_published_at:None}
}
#[tokio::test]
async fn transaction_rollback_leaves_no_snapshot_and_duplicate_is_refused() {
    let state=crate::spaces::properties::tests::property_test_state().await;
    let mut tx=state.db.begin().await.unwrap();
    record(&mut tx,&capture(),"operation","revision","owner-a").await.unwrap();
    tx.rollback().await.unwrap();
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
    let mut tx=state.db.begin().await.unwrap();
    let rows=record(&mut tx,&capture(),"operation","revision","owner-a").await.unwrap();
    verify_record(&mut tx,&rows).await.unwrap();
    assert!(record(&mut tx,&capture(),"operation","revision","owner-a").await.is_err());
    tx.rollback().await.unwrap();
}
#[tokio::test]
async fn sql_post_insert_trigger_tampering_fails_exact_precommit_verification() {
    let state=crate::spaces::properties::tests::property_test_state().await;
    sqlx::query("CREATE TRIGGER damage_snapshot AFTER INSERT ON document_publication_snapshots BEGIN UPDATE document_publication_snapshots SET captured_by='foreign' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
    let mut tx=state.db.begin().await.unwrap();
    let rows=record(&mut tx,&capture(),"operation","revision","owner-a").await.unwrap();
    assert!(verify_record(&mut tx,&rows).await.is_err());tx.rollback().await.unwrap();
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
}
#[tokio::test]
async fn unchanged_values_with_raw_schema_drift_refuse_capture() {
    let state=crate::spaces::properties::tests::property_test_state().await;
    let mut conn=state.db.acquire().await.unwrap();
    let original=comparisons(&mut conn,"space-a","target-a").await.unwrap();
    sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('history-schema','space-a','New definition','text')").execute(&mut *conn).await.unwrap();
    assert!(verify_comparisons(&mut conn,"space-a","target-a",&original).await.is_err());
}
