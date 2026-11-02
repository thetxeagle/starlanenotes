//! Linux bounded-memory, private anonymous download snapshots. The caller owns
//! all path, source, alias and authorization proofs; this is not storage recovery.
//! Other hosts retain the existing whole-file buffering behavior.
use axum::body::Body;
use std::{io, path::Path};

#[derive(Debug)]
pub(super) enum DownloadError {
    Source(io::Error),
    Snapshot(io::Error),
}

#[cfg(target_os = "linux")]
const CHUNK_BYTES: usize = 64 * 1024;

#[cfg(target_os = "linux")]
pub(super) struct Snapshot {
    file: std::fs::File,
    length: u64,
}
#[cfg(not(target_os = "linux"))]
pub(super) struct Snapshot {
    bytes: Vec<u8>,
}

#[cfg(target_os = "linux")]
fn anonymous_file(directory: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    // No directory entry is created. Dropping the last descriptor releases the
    // snapshot, including on capture failures or process interruption.
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_TMPFILE | libc::O_CLOEXEC)
        .open(directory)
}

#[cfg(target_os = "linux")]
async fn capture_into<R, W>(
    source: &mut R,
    target: &mut W,
    expected: u64,
) -> Result<u64, DownloadError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut bytes = vec![0; CHUNK_BYTES];
    let mut remaining = expected;
    while remaining != 0 {
        let capacity = remaining.min(CHUNK_BYTES as u64) as usize;
        let count = source
            .read(&mut bytes[..capacity])
            .await
            .map_err(DownloadError::Source)?;
        if count == 0 {
            return Err(DownloadError::Source(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Attachment shortened during capture",
            )));
        }
        target
            .write_all(&bytes[..count])
            .await
            .map_err(DownloadError::Snapshot)?;
        remaining -= count as u64;
    }
    // Capture may consume at most the opened byte length. Detect growth without
    // copying an unbounded appended tail into temporary storage.
    if source
        .read(&mut bytes[..1])
        .await
        .map_err(DownloadError::Source)?
        != 0
    {
        return Err(DownloadError::Source(io::Error::new(
            io::ErrorKind::InvalidData,
            "Attachment grew during capture",
        )));
    }
    target.flush().await.map_err(DownloadError::Snapshot)?;
    Ok(expected)
}

#[cfg(target_os = "linux")]
async fn capture_reader<R: tokio::io::AsyncRead + Unpin>(
    source: &mut R,
    file: std::fs::File,
    expected: u64,
) -> Result<Snapshot, DownloadError> {
    let mut target = tokio::fs::File::from_std(file);
    let length = capture_into(source, &mut target, expected).await?;
    let file = target.into_std().await;
    Ok(Snapshot { file, length })
}

pub(super) async fn capture(path: &Path) -> Result<Snapshot, DownloadError> {
    capture_bounded(path,u64::MAX).await
}
pub(super) async fn capture_bounded(path:&Path,maximum:u64)->Result<Snapshot,DownloadError>{
    #[cfg(target_os = "linux")]
    {
        let mut source = tokio::fs::File::open(path)
            .await
            .map_err(DownloadError::Source)?;
        let metadata = source.metadata().await.map_err(DownloadError::Source)?;
        if !metadata.is_file() {
            return Err(DownloadError::Source(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Attachment is not a regular file",
            )));
        }
        if metadata.len()>maximum {return Err(DownloadError::Source(io::Error::new(io::ErrorKind::InvalidData,"Attachment exceeds snapshot limit")));}
        let file = anonymous_file(&std::env::temp_dir()).map_err(DownloadError::Snapshot)?;
        capture_reader(&mut source, file, metadata.len()).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(Snapshot {
            bytes: tokio::fs::read(path).await.map_err(DownloadError::Source)?,
        })
    }
}

#[cfg(target_os = "linux")]
fn read_chunk(reader: &mut impl io::Read, bytes: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < bytes.len() {
        match reader.read(&mut bytes[count..]) {
            Ok(0) => break,
            Ok(read) => count += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}

impl Snapshot {
    pub(super) fn length(&self) -> u64 {
        #[cfg(target_os = "linux")]
        {
            self.length
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.bytes.len() as u64
        }
    }

    /// Exact byte comparison and rewind occur synchronously after the caller's
    /// final credentials. There is no additional await before building response.
    pub(super) fn matches_source_and_rewind(&mut self, path: &Path) -> Result<bool, DownloadError> {
        #[cfg(target_os = "linux")]
        {
            use std::io::Seek;
            let mut source = std::fs::File::open(path).map_err(DownloadError::Source)?;
            self.file.rewind().map_err(DownloadError::Snapshot)?;
            let mut captured = vec![0; CHUNK_BYTES];
            let mut current = vec![0; CHUNK_BYTES];
            let matches = loop {
                let left =
                    read_chunk(&mut self.file, &mut captured).map_err(DownloadError::Snapshot)?;
                let right = read_chunk(&mut source, &mut current).map_err(DownloadError::Source)?;
                if left != right || captured[..left] != current[..right] {
                    break false;
                }
                if left == 0 {
                    break true;
                }
            };
            self.file.rewind().map_err(DownloadError::Snapshot)?;
            Ok(matches)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(std::fs::read(path).map_err(DownloadError::Source)? == self.bytes)
        }
    }

    pub(super) fn into_body(self) -> Body {
        #[cfg(target_os = "linux")]
        {
            body(tokio::fs::File::from_std(self.file), self.length)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Body::from(self.bytes)
        }
    }
}

#[cfg(target_os = "linux")]
fn body<R: tokio::io::AsyncRead + Unpin + Send + 'static>(reader: R, length: u64) -> Body {
    use tokio::io::AsyncReadExt;
    Body::from_stream(futures::stream::try_unfold(
        (reader, length),
        |(mut reader, remaining)| async move {
            if remaining == 0 {
                return Ok::<_, io::Error>(None);
            }
            let mut bytes = vec![0; remaining.min(CHUNK_BYTES as u64) as usize];
            let read = reader.read(&mut bytes).await?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Download snapshot ended early",
                ));
            }
            bytes.truncate(read);
            Ok(Some((bytes, (reader, remaining - read as u64))))
        },
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, PermissionsExt},
    };
    use std::{
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        task::{Context, Poll},
    };
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    struct Fixture {
        root: std::path::PathBuf,
        source: std::path::PathBuf,
    }
    impl Fixture {
        fn new(bytes: &[u8]) -> Self {
            let root = std::env::temp_dir().join(format!(
                "starlane-download-snapshot-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir(&root).unwrap();
            let source = root.join("asset.bin");
            std::fs::write(&source, bytes).unwrap();
            Self { root, source }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn copy_snapshot_hash_handle_preserves_complete_bounded_stream() {
        let bytes:Vec<u8>=(0..CHUNK_BYTES*3+17).map(|n|(n%251) as u8).collect();
        let fixture=Fixture::new(&bytes);
        assert!(capture_bounded(&fixture.source,bytes.len() as u64-1).await.is_err());
        let snapshot=capture_bounded(&fixture.source,bytes.len() as u64).await.unwrap();
        let expected=hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
        assert_eq!(snapshot.sha256().unwrap(),expected);
        let mut guard=snapshot.copy_handle().unwrap();
        assert!(guard.matches_source_and_rewind(&fixture.source).unwrap());
        let mut stream=snapshot.into_body().into_data_stream();let mut actual=Vec::new();
        while let Some(chunk)=stream.next().await {let chunk=chunk.unwrap();assert!(chunk.len()<=CHUNK_BYTES);actual.extend_from_slice(&chunk);}
        assert_eq!(actual,bytes);
        assert!(guard.matches_source_and_rewind(&fixture.source).unwrap());
        assert!(guard.matches_sha256(bytes.len() as u64,&expected).unwrap());
        std::fs::write(&fixture.source,b"different source").unwrap();
        assert!(!guard.matches_source_and_rewind(&fixture.source).unwrap());
    }

    fn descriptor_identity(fd: i32) -> Option<(u64, u64)> {
        std::fs::metadata(format!("/proc/self/fd/{fd}"))
            .ok()
            .map(|md| (md.dev(), md.ino()))
    }
    async fn released(fd: i32, identity: (u64, u64)) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while descriptor_identity(fd) == Some(identity) {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("anonymous descriptor was not released");
    }

    struct Probe {
        reads: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
        max_request: Arc<AtomicUsize>,
        fail_after: Option<usize>,
        pending_after: Option<usize>,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl AsyncRead for Probe {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.max_request
                .fetch_max(buffer.remaining(), Ordering::SeqCst);
            let count = self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_after.is_some_and(|limit| count >= limit) {
                return Poll::Ready(Err(io::Error::other("Injected source failure")));
            }
            if self.pending_after.is_some_and(|limit| count >= limit) {
                return Poll::Pending;
            }
            let length = buffer.remaining();
            buffer.initialize_unfilled()[..length].fill(42);
            buffer.advance(length);
            Poll::Ready(Ok(()))
        }
    }
    fn probe(fail_after: Option<usize>, pending_after: Option<usize>) -> Probe {
        Probe {
            reads: Arc::new(AtomicUsize::new(0)),
            drops: Arc::new(AtomicUsize::new(0)),
            max_request: Arc::new(AtomicUsize::new(0)),
            fail_after,
            pending_after,
        }
    }
    struct FailedDisk;
    impl AsyncWrite for FailedDisk {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::from_raw_os_error(libc::ENOSPC)))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn anonymous_snapshot_is_private_and_drop_releases_descriptor_without_artifacts() {
        let fixture = Fixture::new(b"private bytes");
        let snapshot = capture(&fixture.source).await.unwrap();
        let metadata = snapshot.file.metadata().unwrap();
        assert_eq!(metadata.nlink(), 0);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let fd = snapshot.file.as_raw_fd();
        let identity = (metadata.dev(), metadata.ino());
        assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
        drop(snapshot);
        released(fd, identity).await;
        assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn capture_and_exact_final_comparison_rewind_preserve_binary_response_and_empty_file() {
        let bytes: Vec<u8> = (0..CHUNK_BYTES * 3 + 19).map(|i| (i % 256) as u8).collect();
        let fixture = Fixture::new(&bytes);
        let mut snapshot = capture(&fixture.source).await.unwrap();
        assert_eq!(snapshot.length(), bytes.len() as u64);
        assert!(snapshot.matches_source_and_rewind(&fixture.source).unwrap());
        assert_eq!(
            axum::body::to_bytes(snapshot.into_body(), bytes.len())
                .await
                .unwrap()
                .as_ref(),
            bytes
        );
        std::fs::write(&fixture.source, []).unwrap();
        let mut snapshot = capture(&fixture.source).await.unwrap();
        assert_eq!(snapshot.length(), 0);
        assert!(snapshot.matches_source_and_rewind(&fixture.source).unwrap());
        assert!(axum::body::to_bytes(snapshot.into_body(), 0)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn source_mutations_are_exactly_detected_before_response() {
        for change in ["same-size", "truncate", "append", "replace"] {
            let fixture = Fixture::new(b"ORIGINAL");
            let mut snapshot = capture(&fixture.source).await.unwrap();
            match change {
                "same-size" => {
                    std::fs::write(&fixture.source, b"MUTATION").unwrap();
                }
                "truncate" => {
                    std::fs::write(&fixture.source, b"SHORT").unwrap();
                }
                "append" => {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&fixture.source)
                        .unwrap()
                        .write_all(b"APPEND")
                        .unwrap();
                }
                "replace" => {
                    let replacement = fixture.root.join("new");
                    std::fs::write(&replacement, b"REPLACED").unwrap();
                    std::fs::rename(replacement, &fixture.source).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                !snapshot.matches_source_and_rewind(&fixture.source).unwrap(),
                "{change}"
            );
            assert_eq!(
                axum::body::to_bytes(snapshot.into_body(), 8)
                    .await
                    .unwrap()
                    .as_ref(),
                b"ORIGINAL"
            );
        }
    }

    #[tokio::test]
    async fn post_response_source_changes_do_not_change_slow_polled_snapshot() {
        let bytes = vec![19; CHUNK_BYTES * 3 + 7];
        let fixture = Fixture::new(&bytes);
        let mut snapshot = capture(&fixture.source).await.unwrap();
        assert!(snapshot.matches_source_and_rewind(&fixture.source).unwrap());
        let mut stream = snapshot.into_body().into_data_stream();
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.len(), CHUNK_BYTES);
        std::fs::write(&fixture.source, vec![77; bytes.len()]).unwrap();
        let mut received = first.to_vec();
        while let Some(chunk) = stream.next().await {
            tokio::task::yield_now().await;
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, bytes);
    }

    #[tokio::test]
    async fn stream_reads_are_lazy_bounded_and_reader_drops_on_cancellation() {
        let reader = probe(None, None);
        let reads = reader.reads.clone();
        let drops = reader.drops.clone();
        let max = reader.max_request.clone();
        let mut stream = body(reader, 1024 * 1024 * 1024).into_data_stream();
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(stream.next().await.unwrap().unwrap().len(), CHUNK_BYTES);
        tokio::task::yield_now().await;
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(max.load(Ordering::SeqCst), CHUNK_BYTES);
        drop(stream);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_and_cancelled_capture_release_real_anonymous_files() {
        let fixture = Fixture::new(b"source");
        for cancel in [false, true] {
            let file = anonymous_file(&fixture.root).unwrap();
            let fd = file.as_raw_fd();
            let identity = descriptor_identity(fd).unwrap();
            let mut source = if cancel {
                probe(None, Some(1))
            } else {
                probe(Some(1), None)
            };
            let max = source.max_request.clone();
            if cancel {
                let mut operation =
                    Box::pin(capture_reader(&mut source, file, CHUNK_BYTES as u64 * 2));
                assert!(tokio::time::timeout(
                    std::time::Duration::from_millis(25),
                    operation.as_mut()
                )
                .await
                .is_err());
                drop(operation);
            } else {
                assert!(matches!(
                    capture_reader(&mut source, file, CHUNK_BYTES as u64 * 2).await,
                    Err(DownloadError::Source(_))
                ));
            }
            assert_eq!(max.load(Ordering::SeqCst), CHUNK_BYTES);
            released(fd, identity).await;
            assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
        }
    }

    struct PartialDiskFailure {
        file: tokio::fs::File,
        remaining: usize,
    }
    impl AsyncWrite for PartialDiskFailure {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.remaining == 0 {
                return Poll::Ready(Err(io::Error::from_raw_os_error(libc::ENOSPC)));
            }
            let count = self.remaining.min(bytes.len());
            match Pin::new(&mut self.file).poll_write(cx, &bytes[..count]) {
                Poll::Ready(Ok(written)) => {
                    self.remaining -= written;
                    Poll::Ready(Ok(written))
                }
                result => result,
            }
        }
        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.file).poll_flush(cx)
        }
        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.file).poll_shutdown(cx)
        }
    }

    #[tokio::test]
    async fn partial_snapshot_disk_failure_releases_anonymous_partial_inode() {
        let fixture = Fixture::new(b"source");
        let file = anonymous_file(&fixture.root).unwrap();
        let fd = file.as_raw_fd();
        let identity = descriptor_identity(fd).unwrap();
        let mut target = PartialDiskFailure {
            file: tokio::fs::File::from_std(file),
            remaining: 4,
        };
        let mut source = std::io::Cursor::new(b"complete bytes");
        assert!(matches!(capture_into(&mut source,&mut target,14).await,
            Err(DownloadError::Snapshot(error)) if error.raw_os_error()==Some(libc::ENOSPC)));
        drop(target);
        released(fd, identity).await;
        assert_eq!(std::fs::read_dir(&fixture.root).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn temporary_storage_errors_have_distinct_classification() {
        let fixture = Fixture::new(b"source");
        assert!(anonymous_file(&fixture.root.join("missing")).is_err());
        let mut source = std::io::Cursor::new(b"payload");
        let error = capture_into(&mut source, &mut FailedDisk, 7)
            .await
            .unwrap_err();
        assert!(matches!(error,DownloadError::Snapshot(e) if e.raw_os_error()==Some(libc::ENOSPC)));
        assert!(matches!(
            capture(&fixture.root.join("missing")).await,
            Err(DownloadError::Source(_))
        ));
        assert!(matches!(
            capture(&fixture.root).await,
            Err(DownloadError::Source(_))
        ));
    }

    #[tokio::test]
    async fn capture_refuses_premature_eof_and_caps_growing_source_at_opened_length() {
        let fixture = Fixture::new(b"source");
        let mut short = std::io::Cursor::new(vec![1; 3]);
        assert!(
            matches!(capture_into(&mut short, &mut tokio::io::sink(),4).await,
            Err(DownloadError::Source(error)) if error.kind()==io::ErrorKind::UnexpectedEof)
        );
        let file = anonymous_file(&fixture.root).unwrap();
        let held = file.try_clone().unwrap();
        let mut growing = probe(None, None);
        let reads = growing.reads.clone();
        let expected = CHUNK_BYTES as u64 * 2;
        assert!(matches!(capture_reader(&mut growing,file,expected).await,
            Err(DownloadError::Source(error)) if error.kind()==io::ErrorKind::InvalidData));
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        // The failed Tokio write task can finish after return; let it drain before
        // checking the held anonymous inode's bounded size.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(held.metadata().unwrap().len() <= expected);
    }

    #[tokio::test]
    async fn stream_short_reads_and_io_errors_do_not_complete_successfully() {
        let mut stream = body(std::io::Cursor::new(vec![1; 3]), 4).into_data_stream();
        assert_eq!(stream.next().await.unwrap().unwrap().as_ref(), &[1, 1, 1]);
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
        let mut stream = body(probe(Some(1), None), CHUNK_BYTES as u64 * 2).into_data_stream();
        assert!(stream.next().await.unwrap().is_ok());
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
    }
}

#[cfg(target_os="linux")]
impl Snapshot {
    pub(super) fn copy_handle(&self)->Result<Self,DownloadError>{Ok(Self{file:self.file.try_clone().map_err(DownloadError::Snapshot)?,length:self.length})}
    pub(super) fn sha256(&self)->Result<String,DownloadError>{
        use std::os::unix::fs::FileExt;use sha2::{Digest,Sha256};
        let mut hash=Sha256::new();let mut offset=0;let mut bytes=[0u8;CHUNK_BYTES];
        while offset<self.length {let cap=(self.length-offset).min(CHUNK_BYTES as u64) as usize;let count=self.file.read_at(&mut bytes[..cap],offset).map_err(DownloadError::Snapshot)?;if count==0{return Err(DownloadError::Snapshot(io::Error::new(io::ErrorKind::UnexpectedEof,"Snapshot shortened")));}hash.update(&bytes[..count]);offset+=count as u64;}
        Ok(hex::encode(hash.finalize()))
    }
    pub(super) fn matches_sha256(&self,expected:u64,digest:&str)->Result<bool,DownloadError>{
        use std::os::unix::fs::FileExt;use sha2::{Digest,Sha256};
        if self.length!=expected{return Ok(false);}
        let mut hash=Sha256::new();let mut offset=0;let mut bytes=[0u8;CHUNK_BYTES];
        while offset<expected {let cap=(expected-offset).min(CHUNK_BYTES as u64) as usize;let count=self.file.read_at(&mut bytes[..cap],offset).map_err(DownloadError::Snapshot)?;if count==0{return Ok(false);}hash.update(&bytes[..count]);offset+=count as u64;}
        if self.file.read_at(&mut bytes[..1],offset).map_err(DownloadError::Snapshot)?!=0{return Ok(false);}
        Ok(hex::encode(hash.finalize())==digest)
    }
}
