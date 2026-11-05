//! What building a payment asks the node first, kept for a few seconds and
//! asked once.
//!
//! The fee rate and the chain tip are two round trips, and each is a node's
//! half second or more. The Send screen asks them as it opens
//! ([`Wallet::warm_up_send`](crate::Wallet::warm_up_send)), so the payment
//! built a moment later finds them kept; and if it is built while they are
//! still on their way, it waits for that answer instead of asking again.

use std::{
    future::Future,
    time::{Duration, Instant},
};

use monero_wallet::interface::FeeRate;
use tokio::sync::Mutex;

use crate::node::Setting;

/// How long an answer stands in for asking the node again: well under a
/// block's two minutes.
const KEPT_FOR: Duration = Duration::from_secs(20);

/// The fee rate and the chain tip.
pub(crate) type Ready = (FeeRate, usize);

/// The answer, whose node gave it, and when.
struct Kept {
    node: Setting,
    at: Instant,
    ready: Ready,
}

/// The last [`Ready`], kept for the node it came from.
pub struct SendContext {
    kept_for: Duration,
    /// Held while the node is being asked, so whoever comes second finds the
    /// answer here. A caller dropped midway (the warm-up, on leaving the
    /// screen) lets go of it and the next asks for itself.
    kept: Mutex<Option<Kept>>,
}

impl Default for SendContext {
    fn default() -> Self {
        SendContext::new()
    }
}

impl SendContext {
    pub fn new() -> SendContext {
        SendContext::kept_for(KEPT_FOR)
    }

    fn kept_for(kept_for: Duration) -> SendContext {
        SendContext { kept_for, kept: Mutex::new(None) }
    }

    /// The answer kept for `node` if it is fresh, else what `ask` gets (and
    /// keeps). An answer from another node is never taken for this one's.
    pub async fn get<E, F: Future<Output = Result<Ready, E>>>(&self, node: &Setting, ask: impl FnOnce() -> F) -> Result<Ready, E> {
        let mut kept = self.kept.lock().await;
        if let Some(k) = kept.as_ref()
            && k.node == *node
            && k.at.elapsed() < self.kept_for
        {
            return Ok(k.ready);
        }
        let ready = ask().await?;
        *kept = Some(Kept { node: node.clone(), at: Instant::now(), ready });
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn node(url: &str) -> Setting {
        Setting { url: url.into(), self_signed: false }
    }

    fn ready(tip: usize) -> Ready {
        (FeeRate::new(20_000, 10_000).unwrap(), tip)
    }

    /// An asker that counts itself, and takes `wait` to answer.
    async fn asked(count: &AtomicU32, wait: Duration, tip: usize) -> Result<Ready, String> {
        count.fetch_add(1, Ordering::Relaxed);
        tokio::time::sleep(wait).await;
        Ok(ready(tip))
    }

    #[tokio::test]
    async fn a_second_caller_waits_for_the_first_answer_instead_of_asking() {
        let (context, count) = (SendContext::new(), AtomicU32::new(0));
        let n = node("a");
        let slow = Duration::from_millis(200);
        let (first, second, third) = tokio::join!(
            context.get(&n, || asked(&count, slow, 7)),
            context.get(&n, || asked(&count, slow, 8)),
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                context.get(&n, || asked(&count, slow, 9)).await
            },
        );
        assert_eq!((first.unwrap().1, second.unwrap().1, third.unwrap().1), (7, 7, 7));
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn an_answer_is_kept_until_it_is_old() {
        let (context, count) = (SendContext::kept_for(Duration::from_millis(150)), AtomicU32::new(0));
        let n = node("a");
        context.get(&n, || asked(&count, Duration::ZERO, 1)).await.unwrap();
        context.get(&n, || asked(&count, Duration::ZERO, 2)).await.unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 1, "kept");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(context.get(&n, || asked(&count, Duration::ZERO, 3)).await.unwrap().1, 3, "old, so asked again");
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn another_nodes_answer_is_not_taken_for_this_ones() {
        let (context, count) = (SendContext::new(), AtomicU32::new(0));
        context.get(&node("a"), || asked(&count, Duration::ZERO, 1)).await.unwrap();
        let b = context.get(&node("b"), || asked(&count, Duration::ZERO, 2)).await.unwrap();
        assert_eq!((b.1, count.load(Ordering::Relaxed)), (2, 2));
        let other = Setting { url: "b".into(), self_signed: true };
        context.get(&other, || asked(&count, Duration::ZERO, 3)).await.unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 3, "the same address, trusted differently, is another node");
    }

    #[tokio::test]
    async fn a_caller_dropped_midway_lets_the_next_ask_for_itself() {
        let (context, count) = (SendContext::new(), AtomicU32::new(0));
        let n = node("a");
        let dropped = tokio::time::timeout(Duration::from_millis(50), context.get(&n, || asked(&count, Duration::from_millis(500), 1))).await;
        assert!(dropped.is_err());
        let next = context.get(&n, || asked(&count, Duration::ZERO, 2)).await.unwrap();
        assert_eq!((next.1, count.load(Ordering::Relaxed)), (2, 2));
    }

    #[tokio::test]
    async fn a_failure_is_not_kept() {
        let (context, count) = (SendContext::new(), AtomicU32::new(0));
        let n = node("a");
        let failed = context.get(&n, || async { Err("down".to_owned()) }).await;
        assert!(failed.is_err());
        assert_eq!(context.get(&n, || asked(&count, Duration::ZERO, 5)).await.unwrap().1, 5);
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
