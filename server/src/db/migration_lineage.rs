//! Read-only admission before SQLx may create history or execute pending DDL.
//! Canonical prefixes only. Divergent lifecycle49-51 remains a preserved source,
//! never a permit to replace checksums, skip validation or mint a new history.
use super::engine::{selected_migrator, DatabaseEngine};
use anyhow::{bail, ensure};
use sqlx::{AnyPool, Row};

pub(super) async fn preflight(pool: &AnyPool, engine: DatabaseEngine) -> anyhow::Result<()> {
    let mut conn = pool.acquire().await?;
    let objects: Vec<String> = match engine {
        DatabaseEngine::Sqlite => sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type IN ('table','view','trigger') AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut *conn).await?,
        DatabaseEngine::Postgres => sqlx::query_scalar("SELECT c.relname::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind IN ('r','p','v','m','S','f') ORDER BY c.relname").fetch_all(&mut *conn).await?,
    };
    if !objects.iter().any(|name| name == "_sqlx_migrations") {
        ensure!(objects.is_empty(), "Untracked application schema: migration refused without changing source/history");
        return Ok(());
    }
    let rows = sqlx::query("SELECT version, description, checksum, CASE WHEN success THEN 1 ELSE 0 END AS succeeded FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&mut *conn).await?;
    // An empty history alongside tables is not a fresh database or a repair grant.
    ensure!(!rows.is_empty() || objects == ["_sqlx_migrations"], "Empty migration history with existing application tables: migration refused");
    let known = selected_migrator(engine).iter().collect::<Vec<_>>();
    ensure!(rows.len() <= known.len(), "Unknown migration lineage: preserve original database and history");
    for (row, expected) in rows.iter().zip(known) {
        let version: i64 = row.try_get("version")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;
        if engine == DatabaseEngine::Sqlite && (49..=51).contains(&version) {
            use sha2::{Digest, Sha384};
            let legacy = [
                include_bytes!("../../migration-lineages/lifecycle-3df4c91b/0049_draft_publication_receipts.sql").as_slice(),
                include_bytes!("../../migration-lineages/lifecycle-3df4c91b/0050_document_publication_snapshots.sql").as_slice(),
                include_bytes!("../../migration-lineages/lifecycle-3df4c91b/0051_snapshot_recovery_receipts.sql").as_slice(),
            ];
            if checksum == &Sha384::digest(legacy[(version - 49) as usize])[..] {
                bail!("Legacy lifecycle SQLite49-51 lineage: preserve this database; canonical in-place upgrade is unavailable; no migration/history repair was attempted");
            }
        }
        ensure!(version == expected.version && checksum.as_slice() == expected.checksum.as_ref()
            && row.try_get::<String,_>("description")? == expected.description.as_ref()
            && row.try_get::<i64,_>("succeeded")? == 1,
            "Non-prefix, failed or changed migration lineage: preserve original database/history; upgrade refused before pending DDL");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::migrate::Migrator;
    use std::borrow::Cow;
    async fn sqlite() -> AnyPool {
        sqlx::any::install_default_drivers();
        sqlx::any::AnyPoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap()
    }
    async fn prefix(pool: &AnyPool, engine: DatabaseEngine, count: usize) {
        let migrations = selected_migrator(engine).iter().take(count).cloned().collect();
        Migrator { migrations: Cow::Owned(migrations), ignore_missing:false,locking:true,no_tx:false }.run(pool).await.unwrap();
    }
    async fn state(pool: &AnyPool) -> Vec<(i64,String,Vec<u8>,String,i64,i64)> {
        sqlx::query_as("SELECT version,description,checksum,CAST(installed_on AS TEXT),CASE WHEN success THEN 1 ELSE 0 END,execution_time FROM _sqlx_migrations ORDER BY version").fetch_all(pool).await.unwrap()
    }
    #[tokio::test]
    async fn bridge_fresh_and_canonical_upgrade_preserve_history_and_durable_rows() {
        for old in [0, 53] {
            let pool=sqlite().await;
            if old>0 {prefix(&pool,DatabaseEngine::Sqlite,old).await;}
            let before=if old>0 {state(&pool).await}else{vec![]};
            super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await.unwrap();
            let after=state(&pool).await;assert_eq!(&after[..before.len()],before.as_slice());assert_eq!(after.len(),57);
            sqlx::query("INSERT INTO draft_publication_receipts VALUES('operation','actor','space','page','draft','fingerprint','{}','{}','{}','{}','{}')").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO document_publication_snapshots VALUES('snapshot','page','space','publication','operation','operation','capture_v2','{}','digest',$1,'sha',4,'published','captured','actor')").bind(vec![0u8,1,254,255]).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO snapshot_recovery_receipts VALUES('recover','actor','space','page','snapshot','{}','{}','digest')").execute(&pool).await.unwrap();
            let rows:Vec<u8>=sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE id='snapshot'").fetch_one(&pool).await.unwrap();
            super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await.unwrap();
            assert_eq!(state(&pool).await,after);assert_eq!(rows,vec![0,1,254,255]);
            for table in ["draft_publication_receipts","document_publication_snapshots","snapshot_recovery_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&pool).await.unwrap(),1);}
        }
    }
    #[tokio::test]
    async fn bridge_refuses_legacy_unknown_gap_checksum_and_failed_history_without_writes() {
        for case in ["legacy","unknown","gap","checksum","failed"] {
            let pool=sqlite().await;prefix(&pool,DatabaseEngine::Sqlite,53).await;
            match case {
                "legacy"=>{use sha2::{Digest,Sha384};sqlx::query("UPDATE _sqlx_migrations SET checksum=$1,description='draft publication receipts' WHERE version=49").bind(Sha384::digest(include_bytes!("../../migration-lineages/lifecycle-3df4c91b/0049_draft_publication_receipts.sql")).to_vec()).execute(&pool).await.unwrap();},
                "unknown"=>{sqlx::query("UPDATE _sqlx_migrations SET version=999 WHERE version=53").execute(&pool).await.unwrap();},
                "gap"=>{sqlx::query("DELETE FROM _sqlx_migrations WHERE version=2").execute(&pool).await.unwrap();},
                "checksum"=>{sqlx::query("UPDATE _sqlx_migrations SET checksum=X'00' WHERE version=53").execute(&pool).await.unwrap();},
                _=>{sqlx::query("UPDATE _sqlx_migrations SET success=FALSE WHERE version=53").execute(&pool).await.unwrap();},
            }
            let before=state(&pool).await;
            let schema:Vec<(String,String)>=sqlx::query_as("SELECT name,COALESCE(sql,'') FROM sqlite_master ORDER BY name").fetch_all(&pool).await.unwrap();
            assert!(super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await.is_err(),"{case}");
            assert_eq!(state(&pool).await,before,"{case}");
            assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT name,COALESCE(sql,'') FROM sqlite_master ORDER BY name").fetch_all(&pool).await.unwrap(),schema,"{case}");
        }
    }
    #[tokio::test]
    async fn bridge_genuine_legacy_lifecycle_history_and_snapshot_are_preserved_on_refusal() {
        use sqlx::migrate::{Migration,MigrationType};
        let pool=sqlite().await;prefix(&pool,DatabaseEngine::Sqlite,48).await;
        let legacy=[(49,"draft publication receipts",include_str!("../../migration-lineages/lifecycle-3df4c91b/0049_draft_publication_receipts.sql")),(50,"document publication snapshots",include_str!("../../migration-lineages/lifecycle-3df4c91b/0050_document_publication_snapshots.sql")),(51,"snapshot recovery receipts",include_str!("../../migration-lineages/lifecycle-3df4c91b/0051_snapshot_recovery_receipts.sql"))];
        let mut migrations=selected_migrator(DatabaseEngine::Sqlite).iter().take(48).cloned().collect::<Vec<_>>();
        migrations.extend(legacy.into_iter().map(|(version,description,sql)|Migration::new(version,Cow::Borrowed(description),MigrationType::Simple,Cow::Borrowed(sql),false)));
        Migrator{migrations:Cow::Owned(migrations),ignore_missing:false,locking:true,no_tx:false}.run(&pool).await.unwrap();
        sqlx::query("INSERT INTO document_publication_snapshots VALUES('original','page','space','baseline','capture',NULL,'legacy_content_v1','{}','digest',$1,'sha',4,'published','captured','actor')").bind(vec![0u8,1,254,255]).execute(&pool).await.unwrap();
        let before=state(&pool).await;
        let error=super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await.unwrap_err();
        assert!(error.to_string().contains("Legacy lifecycle SQLite49-51"));assert_eq!(state(&pool).await,before);
        assert_eq!(sqlx::query_scalar::<_,Vec<u8>>("SELECT raw_markdown FROM document_publication_snapshots").fetch_one(&pool).await.unwrap(),vec![0,1,254,255]);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_master WHERE name='attachment_upload_receipts'").fetch_one(&pool).await.unwrap(),0);
    }
    #[tokio::test]
    async fn bridge_untracked_schema_refused_before_history_creation() {
        for ddl in ["CREATE TABLE retained_original(value TEXT)", "CREATE VIEW retained_original AS SELECT 1 AS value"] {
            let pool=sqlite().await;sqlx::query(ddl).execute(&pool).await.unwrap();
            assert!(super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await.is_err());
            let objects:Vec<(String,String)>=sqlx::query_as("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&pool).await.unwrap();
            assert_eq!(objects,vec![("retained_original".to_owned(),ddl.to_owned())]);
        }
    }
    #[tokio::test]
    async fn bridge_empty_ledger_only_bootstraps_without_application_objects() {
        for with_view in [false,true] {
            let pool=sqlite().await;
            sqlx::query("CREATE TABLE _sqlx_migrations(version BIGINT PRIMARY KEY,description TEXT NOT NULL,installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,success BOOLEAN NOT NULL,checksum BLOB NOT NULL,execution_time BIGINT NOT NULL)").execute(&pool).await.unwrap();
            if with_view {sqlx::query("CREATE VIEW retained_original AS SELECT 1 AS value").execute(&pool).await.unwrap();}
            let before:Vec<(String,String)>=sqlx::query_as("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&pool).await.unwrap();
            let result=super::super::engine::migrate_selected(&pool,DatabaseEngine::Sqlite).await;
            if with_view {
                assert!(result.is_err());assert!(state(&pool).await.is_empty());
                assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&pool).await.unwrap(),before);
            } else {result.unwrap();assert_eq!(state(&pool).await.len(),57);}
        }
    }
    #[tokio::test]
    #[ignore = "requires explicit owned REPLAY_CAPACITY_TEST_PG_URL; creates only unique bridge_ schemas"]
    async fn bridge_postgres_owned_fresh_upgrade_retention_and_refusals() -> anyhow::Result<()> {
        let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL")?;
        sqlx::any::install_default_drivers();
        let admin=sqlx::PgPool::connect(&url).await?;
        for case in ["fresh","upgrade","unknown","gap","checksum","failed","empty_view","empty_sequence","view_only"] {
            let schema=format!("bridge_{}",uuid::Uuid::new_v4().simple());
            sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await?;
            let selected=schema.clone();
            let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).after_connect(move|conn,_|{let query=format!("SET search_path TO {selected}");Box::pin(async move{sqlx::query(&query).execute(conn).await?;Ok(())})}).connect(&url).await?;
            if matches!(case,"empty_view"|"empty_sequence"|"view_only") {
                if case!="view_only" {sqlx::query("CREATE TABLE _sqlx_migrations(version BIGINT PRIMARY KEY,description TEXT NOT NULL,installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),success BOOLEAN NOT NULL,checksum BYTEA NOT NULL,execution_time BIGINT NOT NULL)").execute(&pool).await?;}
                sqlx::query(if case=="empty_sequence" {"CREATE SEQUENCE retained_original"} else {"CREATE VIEW retained_original AS SELECT 1 AS value"}).execute(&pool).await?;
                let census="SELECT c.relname::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind IN ('r','p','v','m','S','f') ORDER BY c.relname";
                let before:Vec<String>=sqlx::query_scalar(census).fetch_all(&pool).await?;
                ensure!(super::super::engine::migrate_selected(&pool,DatabaseEngine::Postgres).await.is_err(),"Untracked objects admitted");
                ensure!(sqlx::query_scalar::<_,String>(census).fetch_all(&pool).await?==before,"Refusal changed original objects");
                if case!="view_only" {ensure!(state(&pool).await.is_empty(),"Refusal wrote migration history");}
                pool.close().await;
                sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await?;
                continue;
            }

            if case!="fresh" {prefix(&pool,DatabaseEngine::Postgres,7).await;}
            if !matches!(case,"fresh"|"upgrade") {
                let query=match case {"unknown"=>"UPDATE _sqlx_migrations SET version=999 WHERE version=7","gap"=>"DELETE FROM _sqlx_migrations WHERE version=2","checksum"=>"UPDATE _sqlx_migrations SET checksum=decode('00','hex') WHERE version=7",_=>"UPDATE _sqlx_migrations SET success=FALSE WHERE version=7"};
                sqlx::query(query).execute(&pool).await?;
                let before=state(&pool).await;
                let names:Vec<String>=sqlx::query_scalar("SELECT tablename::text FROM pg_catalog.pg_tables WHERE schemaname=current_schema() ORDER BY tablename").fetch_all(&pool).await?;
                ensure!(super::super::engine::migrate_selected(&pool,DatabaseEngine::Postgres).await.is_err(),"Malformed PG history accepted");
                ensure!(state(&pool).await==before,"Refusal changed original PG history");
                ensure!(sqlx::query_scalar::<_,String>("SELECT tablename::text FROM pg_catalog.pg_tables WHERE schemaname=current_schema() ORDER BY tablename").fetch_all(&pool).await?==names,"Refusal executed pending PG DDL");
            }else{
                let before=if case=="upgrade" {state(&pool).await}else{vec![]};
                super::super::engine::migrate_selected(&pool,DatabaseEngine::Postgres).await?;
                let after=state(&pool).await;ensure!(after.len()==9&&after[..before.len()]==before,"PG original prefix changed");
                sqlx::query("INSERT INTO draft_publication_receipts VALUES('operation','actor','space','page','draft','fingerprint','{}','{}','{}','{}','{}')").execute(&pool).await?;
                sqlx::query("INSERT INTO document_publication_snapshots VALUES('snapshot','page','space','publication','operation','operation','capture_v2','{}','digest',$1,'sha',4,'published','captured','actor')").bind(vec![0u8,1,254,255]).execute(&pool).await?;
                sqlx::query("INSERT INTO snapshot_recovery_receipts VALUES('recover','actor','space','page','snapshot','{}','{}','digest')").execute(&pool).await?;
                super::super::engine::migrate_selected(&pool,DatabaseEngine::Postgres).await?;
                ensure!(state(&pool).await==after,"PG second startup changed migration rows");
                ensure!(sqlx::query_scalar::<_,Vec<u8>>("SELECT raw_markdown FROM document_publication_snapshots").fetch_one(&pool).await?==vec![0,1,254,255],"PG binary snapshot changed");
                for table in ["draft_publication_receipts","document_publication_snapshots","snapshot_recovery_receipts"]{ensure!(sqlx::query_scalar::<_,i64>(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&pool).await?==1,"PG durable receipt absent");}
            }
            pool.close().await;
            sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await?;
        }
        admin.close().await;Ok(())
    }

}
