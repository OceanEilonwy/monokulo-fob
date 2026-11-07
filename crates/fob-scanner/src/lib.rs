//! Finds the blocks that may hold a wallet's money.
//!
//! The prefilter that does nearly all of it lives in `monokulo-8ar` (decisions
//! 359, 360), shared with monokulo-lws: 8aR for each transaction key on the
//! GPU, and a view tag for each output, flagging the few blocks that could pay
//! the wallet. Without a GPU that can do it, every block goes to monero-oxide's
//! `Scanner`. What it exports is re-exported here, so the rest of Fob doesn't
//! depend on it directly.
//!
//! What stays is Fob's: [`scan`], which fans a batch out over the cores and has
//! the `Scanner` read each flagged block (or every block, unfiltered).

pub mod scan;

pub use monokulo_8ar::{Engine, Prefilter, Reason, check_wallet, engine, gpu, gpu_failed, gpu_identity, gpu_ready, rest, set_cache_dir, settle, startup_vectors, wallet_vectors, warm_up};

#[cfg(test)]
mod tests;
