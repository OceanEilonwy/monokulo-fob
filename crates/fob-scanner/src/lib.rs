//! Finds the blocks that may hold a wallet's money.
//!
//! The prefilter that does nearly all of it lives in `monokulo-8ar` (decision
//! 359), shared with monokulo-lws: 8aR for each transaction key and a view tag
//! for each output, on the GPU or the CPU's cores, flagging the few blocks that
//! could pay the wallet. Everything it exports is re-exported here, so the rest
//! of Fob doesn't know it moved.
//!
//! What stays is Fob's: [`scan`], which fans a batch out over the cores and has
//! monero-oxide's `Scanner` read each flagged block, and `policy.rs`, Fob's
//! choice of where Auto hands over to the GPU.

pub mod scan;
mod policy;

pub use monokulo_8ar::{Mode, Prefilter, check_wallet, fe, gpu, gpu_failed, gpu_identity, gpu_ready, mode, rest, set_cache_dir, set_mode, startup_vectors, wallet_vectors, warm_up};

#[cfg(test)]
mod tests;
