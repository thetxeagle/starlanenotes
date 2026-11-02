//! Bounded parser delivery with at most one retained upstream frame.
//! Upstream frame allocation precedes this adapter; no absolute RSS guarantee.
use axum::body::{Body, Bytes};
use futures::Stream;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
pub(super) const DELIVERY_CHUNK: usize = 64 * 1024;
const OVERHEAD: u64 = 64 * 1024;
struct Budget {
    source: Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>,
    remaining: u64,
    held: Option<Bytes>,
    yield_pending: bool,
    failed: bool,
}
impl Stream for Budget {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let s = self.get_mut();
        if s.failed {
            return Poll::Ready(None);
        }
        if s.yield_pending {
            s.yield_pending = false;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if s.held.is_none() {
            match s.source.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(_))) => {
                    s.failed = true;
                    return Poll::Ready(Some(Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "Upload body unavailable",
                    ))));
                }
                Poll::Ready(Some(Ok(b))) => {
                    if b.len() as u64 > s.remaining {
                        s.failed = true;
                        return Poll::Ready(Some(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Upload transport budget exceeded",
                        ))));
                    }
                    s.remaining -= b.len() as u64;
                    s.held = Some(b);
                }
            }
        }
        let held = s.held.as_mut().unwrap();
        let out = held.split_to(held.len().min(DELIVERY_CHUNK));
        if held.is_empty() {
            s.held = None;
        }
        s.yield_pending = true;
        Poll::Ready(Some(Ok(out)))
    }
}
pub(super) fn bounded(body: Body, maximum: usize) -> Body {
    Body::from_stream(Budget {
        source: Box::pin(body.into_data_stream()),
        remaining: (maximum as u64).saturating_add(OVERHEAD),
        held: None,
        yield_pending: false,
        failed: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt};
    #[tokio::test]
    async fn oversized_processed_body_is_rejected() {
        let b = bounded(Body::from(vec![0; DELIVERY_CHUNK + 2]), 1);
        assert!(axum::body::to_bytes(b, usize::MAX).await.is_err());
        let b = bounded(
            Body::from_stream(stream::iter([
                Ok::<_, io::Error>(Bytes::from(vec![0; DELIVERY_CHUNK])),
                Ok(Bytes::from(vec![0; DELIVERY_CHUNK])),
            ])),
            1,
        );
        assert!(axum::body::to_bytes(b, usize::MAX).await.is_err());
    }
    #[tokio::test]
    async fn valid_large_single_upstream_frame_is_split_without_rejection() {
        let expected = vec![0x55; 4 * 1024 * 1024];
        let mut source = bounded(Body::from(expected.clone()), expected.len()).into_data_stream();
        let mut actual = vec![];
        while let Some(chunk) = source.next().await {
            let chunk = chunk.unwrap();
            assert!(chunk.len() <= DELIVERY_CHUNK);
            actual.extend_from_slice(&chunk);
        }
        assert_eq!(actual, expected);
    }
    #[test]
    fn continuously_ready_source_yields_after_one_delivery_chunk() {
        let b = Body::from_stream(stream::iter(
            (0..100).map(|_| Ok::<_, io::Error>(Bytes::from(vec![0; 1024]))),
        ));
        let mut budget = Budget {
            source: Box::pin(b.into_data_stream()),
            remaining: 1_000_000,
            held: None,
            yield_pending: false,
            failed: false,
        };
        let w = futures::task::noop_waker();
        let mut cx = Context::from_waker(&w);
        assert!(matches!(
            Pin::new(&mut budget).poll_next(&mut cx),
            Poll::Ready(Some(Ok(_)))
        ));
        assert!(matches!(
            Pin::new(&mut budget).poll_next(&mut cx),
            Poll::Pending
        ));
        assert!(matches!(
            Pin::new(&mut budget).poll_next(&mut cx),
            Poll::Ready(Some(Ok(_)))
        ));
    }
}
