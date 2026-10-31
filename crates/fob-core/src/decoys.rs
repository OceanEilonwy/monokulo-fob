//! Decoys for sending, without downloading the chain's output distribution
//! every time.
//!
//! Choosing a ring's decoys needs the cumulative count of RingCT outputs at
//! every block since RingCT began: 8 bytes a block, uncompressed, about 18 MB
//! on stagenet and 30 MB on mainnet. monero-oxide asks the node for all of it
//! on each payment, which took minutes from a public node. This keeps it, on
//! disk beside the wallet (it's public chain data), and fetches only the
//! blocks since, plus the last few again in case they were reorganised.

use std::{
    ops::{Bound, RangeBounds},
    path::PathBuf,
    sync::Arc,
};

use monero_wallet::{
    ed25519::Point,
    interface::{EvaluateUnlocked, InterfaceError, ProvidesBlockchainMeta, ProvidesUnvalidatedDecoys, TransactionsError},
};
use tokio::sync::Mutex;

use crate::transport::Daemon;

/// Blocks fetched again at the end of the cached distribution, in case a
/// reorganisation changed them.
const REFETCH: usize = 30;

/// The cumulative RingCT output count at each block from `start`.
struct Distribution {
    start: usize,
    counts: Vec<u64>,
}

impl Distribution {
    fn end(&self) -> Option<usize> {
        (self.start + self.counts.len()).checked_sub(1)
    }

    fn read(path: &PathBuf) -> Option<Distribution> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 8 || bytes.len() % 8 != 0 {
            return None;
        }
        let mut words = bytes.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().expect("8 bytes")));
        let start = usize::try_from(words.next()?).ok()?;
        Some(Distribution { start, counts: words.collect() })
    }

    fn write(&self, path: &PathBuf) {
        let mut bytes = Vec::with_capacity(8 * (self.counts.len() + 1));
        bytes.extend((self.start as u64).to_le_bytes());
        for c in &self.counts {
            bytes.extend(c.to_le_bytes());
        }
        let tmp = path.with_extension("tmp");
        // A cache: if it can't be written, the next payment fetches it again.
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

/// The node, with the output distribution kept between payments.
#[derive(Clone)]
pub struct Decoys {
    daemon: Daemon,
    cache: Arc<Mutex<Option<Distribution>>>,
    file: Arc<PathBuf>,
}

impl Decoys {
    pub fn new(daemon: Daemon, file: PathBuf) -> Decoys {
        let cached = Distribution::read(&file);
        Decoys { daemon, cache: Arc::new(Mutex::new(cached)), file: Arc::new(file) }
    }

    /// Brings the distribution up to block `to`, fetching only what's new.
    pub async fn update(&self, to: usize) -> Result<(), InterfaceError> {
        let mut cache = self.cache.lock().await;
        let from = match cache.as_ref().and_then(Distribution::end) {
            Some(end) if end >= to => return Ok(()),
            Some(end) => (end + 1).saturating_sub(REFETCH).max(cache.as_ref().map_or(0, |d| d.start)),
            None => 0,
        };
        let fresh = ProvidesUnvalidatedDecoys::ringct_output_distribution(&self.daemon, from..=to).await?;
        // The node starts the distribution where RingCT began, if that's after `from`.
        let fresh_start = (to + 1).checked_sub(fresh.len()).ok_or_else(|| InterfaceError::InvalidInterface("too long a distribution".into()))?;
        match cache.as_mut() {
            Some(d) if fresh_start >= d.start && fresh_start <= d.start + d.counts.len() => {
                d.counts.truncate(fresh_start - d.start);
                d.counts.extend(fresh);
            }
            _ => *cache = Some(Distribution { start: fresh_start, counts: fresh }),
        }
        if let Some(d) = cache.as_ref() {
            d.write(&self.file);
        }
        Ok(())
    }
}

impl ProvidesBlockchainMeta for Decoys {
    fn latest_block_number(&self) -> impl Send + Future<Output = Result<usize, InterfaceError>> {
        self.daemon.latest_block_number()
    }
}

impl ProvidesUnvalidatedDecoys for Decoys {
    fn ringct_output_distribution(
        &self,
        range: impl Send + RangeBounds<usize>,
    ) -> impl Send + Future<Output = Result<Vec<u64>, InterfaceError>> {
        let from = match range.start_bound() {
            Bound::Included(f) => Some(*f),
            Bound::Excluded(f) => f.checked_add(1),
            Bound::Unbounded => Some(0),
        };
        let to = match range.end_bound() {
            Bound::Included(t) => Some(Some(*t)),
            Bound::Excluded(t) => Some(t.checked_sub(1)),
            Bound::Unbounded => None,
        };
        async move {
            let bad = || InterfaceError::InternalError("malformed range".into());
            let from = from.ok_or_else(bad)?;
            let to = match to {
                Some(t) => t.ok_or_else(bad)?,
                None => self.daemon.latest_block_number().await?,
            };
            self.update(to).await?;
            let cache = self.cache.lock().await;
            let d = cache.as_ref().ok_or_else(|| InterfaceError::InternalError("no distribution".into()))?;
            if to < d.start {
                return Ok(vec![]);
            }
            let lo = from.max(d.start) - d.start;
            let hi = to - d.start;
            Ok(d.counts.get(lo..=hi).ok_or_else(bad)?.to_vec())
        }
    }

    fn unlocked_ringct_outputs(
        &self,
        indexes: &[u64],
        evaluate_unlocked: EvaluateUnlocked,
    ) -> impl Send + Future<Output = Result<Vec<Option<[Point; 2]>>, TransactionsError>> {
        ProvidesUnvalidatedDecoys::unlocked_ringct_outputs(&self.daemon, indexes, evaluate_unlocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_file_round_trips() {
        let path = std::env::temp_dir().join(format!("fob-decoys-{}", std::process::id()));
        Distribution { start: 1_220_516, counts: vec![1, 5, 9, 14] }.write(&path);
        let back = Distribution::read(&path).unwrap();
        assert_eq!(back.end(), Some(1_220_519));
        assert_eq!((back.start, back.counts), (1_220_516, vec![1, 5, 9, 14]));
        std::fs::write(&path, [1, 2, 3]).unwrap();
        assert!(Distribution::read(&path).is_none(), "a torn file is ignored");
        std::fs::remove_file(path).unwrap();
    }
}
