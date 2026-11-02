// Include beside Root in restoration's Linux implementation. These ignored
// controls prove ONLY SQL byte-lock exclusion by the actual original RO VFS.
// There is deliberately NO read lease, history owner, capture, certificate,
// restored-source actor, or application-authority claim in this fixture.
#[cfg(test)]
mod original_source_sql_lock_tests {
    use super::*;
    use anyhow::{ensure, Context};
    use std::{collections::BTreeMap, os::unix::fs::DirBuilderExt, process::Stdio};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    const ORIGINAL: &str = "original source — Ω\r\nend\n";
    const SETUP: &str = r#"
import os, sqlite3, sys
os.umask(0o077)
c = sqlite3.connect(sys.argv[1])
mode = sys.argv[2]
assert c.execute('PRAGMA journal_mode=' + mode).fetchone()[0] == mode
if mode == 'wal':
    c.execute('PRAGMA wal_autocheckpoint=0')
c.execute('CREATE TABLE original_evidence(id INTEGER PRIMARY KEY, value TEXT NOT NULL)')
c.execute('INSERT INTO original_evidence VALUES(1, ?)', ('original source — Ω\r\nend\n',))
c.commit()
# Materialize a valid last-commit read mark using a genuine SQLite read. The
# source is stopped, with a nonempty committed WAL; no SHM bytes are fabricated.
assert c.execute('SELECT value FROM original_evidence WHERE id=1').fetchone()[0] == 'original source — Ω\r\nend\n'
os._exit(0)
"#;
    const FOREIGN: &str = r#"
import sqlite3, sys
c = sqlite3.connect(sys.argv[1], timeout=0.05)
# Open/read the actual WAL and finalize its read statement BEFORE the parent
# baseline. Root's Unix sidecar-open fchown must not be mistaken for a later
# commit attempt, and no writable source descriptors are hidden from capture:
# this fixture intentionally tests SQL byte locks WITHOUT capture/read leases.
cur = c.execute('SELECT value FROM original_evidence WHERE id=1')
assert cur.fetchone()[0] == 'original source — Ω\r\nend\n'
cur.close()
print('FOREIGN_SQL_CONNECTION_READY', flush=True)
assert sys.stdin.readline() == 'GO\n'
try:
    c.execute('BEGIN IMMEDIATE')
    c.execute('UPDATE original_evidence SET value=? WHERE id=1', ('foreign committed replacement',))
    c.commit()
except sqlite3.OperationalError as error:
    if 'locked' not in str(error).lower():
        raise
    c.rollback()
    c.close()
    print('FOREIGN_PROCESS_COMMIT_BLOCKED:locked', flush=True)
    sys.exit(10)
print('FOREIGN_PROCESS_COMMIT_ADMITTED', flush=True)
sys.exit(42)
"#;

    fn exact_files(source: &Path) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
        let mut files = BTreeMap::new();
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            ensure!(entry.file_type()?.is_file(), "Unexpected SQL-lock fixture entry");
            let name = entry.file_name().into_string().map_err(|_| anyhow::anyhow!("NonUTF8 SQL-lock fixture entry"))?;
            files.insert(name, std::fs::read(entry.path())?);
        }
        Ok(files)
    }

    async fn original_sql_lock(journal: &str) -> anyhow::Result<()> {
        // Same trusted root-owned interpreter admission as the actual producer.
        let (python, _python_pin, _python_stamp, _python_sha) = sqlite_capture_python()?;
        let base = std::env::temp_dir().join(format!("original-readonly-sql-lock-{journal}-{}", uuid::Uuid::new_v4()));
        let source = base.join("source");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&source)?;
        let database = source.join("starlane-notes.db");
        let setup = tokio::process::Command::new(&python)
            .args(["-I", "-c", SETUP]).arg(&database).arg(journal)
            .stdin(Stdio::null()).kill_on_drop(true).output().await?;
        ensure!(setup.status.success(), "Actual {journal} setup failed: status={:?}, stderr={}", setup.status.code(), String::from_utf8_lossy(&setup.stderr));
        let mut foreign = tokio::process::Command::new(&python)
            .args(["-I", "-c", FOREIGN]).arg(&database)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .kill_on_drop(true).spawn()?;
        let mut foreign_input = foreign.stdin.take().context("Foreign SQL control stdin absent")?;
        let mut foreign_output = BufReader::new(foreign.stdout.take().context("Foreign SQL control stdout absent")?);
        let mut ready = String::new();
        tokio::time::timeout(std::time::Duration::from_secs(10), foreign_output.read_line(&mut ready)).await
            .context("Foreign SQL connection did not reach READY")??;
        ensure!(ready == "FOREIGN_SQL_CONNECTION_READY\n", "Foreign SQL connection failed before baseline: {ready:?}");
        // Baseline follows the real preopened writer's WAL access. Subsequent
        // GO triggers a new transaction, not another sidecar open/fchown.
        let before = exact_files(&source)?;
        ensure!(before.contains_key("starlane-notes.db"), "Actual original main is absent");
        if journal == "wal" {
            ensure!(before.get("starlane-notes.db-wal").is_some_and(|bytes| bytes.len() > 32), "Actual WAL must contain committed frames");
            ensure!(before.get("starlane-notes.db-shm").is_some_and(|bytes| !bytes.is_empty()), "Actual stopped WAL must retain existing SHM");
        } else {
            ensure!(!before.contains_key("starlane-notes.db-wal") && !before.contains_key("starlane-notes.db-shm"), "DELETE fixture cannot substitute WAL exclusion");
        }
        let root = Root::open(&source)?;
        let sidecar = |name: &str| -> io::Result<Option<File>> {
            match File::open(source.join(name)) {
                Ok(file) => Ok(Some(file)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        };
        let reader = crate::storage::restoration_sqlite_source_vfs::OriginalSqliteSourceVfs::new(
            root.fd().try_clone()?, source.clone(), File::open(&database)?,
            sidecar("starlane-notes.db-wal")?, sidecar("starlane-notes.db-shm")?, sidecar("starlane-notes.db-journal")?,
            // Intentionally not original capture custody. This tests actual SQL
            // locks even when a foreign process can open the source READWRITE.
            std::sync::Arc::new(|| Ok(())),
        )?;
        ensure!(reader.has_wal_shm_exclusion(), "Selected mode requires actual independent byte-lock exclusion");
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&reader.url()?).await
            .with_context(|| format!("Actual original {journal} readonly VFS connection"))?;
        let first: String = sqlx::query_scalar("SELECT value FROM original_evidence WHERE id=1").fetch_one(&pool).await
            .with_context(|| format!("Actual original {journal} read before foreign writer"))?;
        ensure!(first == ORIGINAL, "Original readonly row differs before foreign attempt");
        ensure!(exact_files(&source)? == before, "Original readonly {journal} pager changed source bytes or entries");

        // Closing an unrelated same-inode inspection FD releases SQLite's
        // process-associated locks on Linux. The independent OFD locks MUST
        // still stop the real foreign SQL commit afterwards.
        let inspection = File::open(&database)?;
        ensure!(physical_stamp(&inspection)?.length == before["starlane-notes.db"].len() as u64, "Inspection selected the wrong original main");
        drop(inspection);
        foreign_input.write_all(b"GO\n").await?;
        foreign_input.shutdown().await?;
        drop(foreign_input);
        let mut stdout = String::new();
        let (foreign_result, output_result) = tokio::join!(foreign.wait_with_output(), foreign_output.read_to_string(&mut stdout));
        let foreign = foreign_result?;
        output_result?;
        ensure!(foreign.status.code() == Some(10) && stdout.contains("FOREIGN_PROCESS_COMMIT_BLOCKED:locked"),
            "Actual {journal} foreign SQL writer was not refused as locked: status={:?}, stdout={}, stderr={}",
            foreign.status.code(), stdout, String::from_utf8_lossy(&foreign.stderr));
        ensure!(exact_files(&source)? == before, "Refused {journal} SQL writer changed original main/WAL/SHM/journal bytes or entries");
        reader.fence()?;
        let after: String = sqlx::query_scalar("SELECT value FROM original_evidence WHERE id=1").fetch_one(&pool).await
            .with_context(|| format!("Actual original {journal} read after refused SQL commit"))?;
        ensure!(after == ORIGINAL, "Refused SQL writer changed original row");
        reader.stop_admissions()?;
        pool.close().await;
        reader.native_close_ack().await?;
        reader.fence()?;
        ensure!(exact_files(&source)? == before, "Readonly {journal} retirement changed original source");
        reader.retire()?;
        println!("ACTUAL_ORIGINAL_READONLY_{journal}_SQL_COMMIT_EXCLUSION_AFTER_INSPECTION_CLOSE_EXACT_SOURCE_PASS (not capture certification)");
        Ok(())
    }

    #[tokio::test]
    #[ignore = "actual trusted host Python, original DELETE SQL byte locks; no capture certification"]
    async fn readonly_original_delete_sql_commit_blocked_after_inspection_fd_close() -> anyhow::Result<()> {
        original_sql_lock("delete").await
    }

    #[tokio::test]
    #[ignore = "actual trusted host Python, retained cold WAL+SHM SQL byte locks; no capture certification"]
    async fn readonly_original_wal_sql_commit_blocked_after_inspection_fd_close() -> anyhow::Result<()> {
        original_sql_lock("wal").await
    }
}
