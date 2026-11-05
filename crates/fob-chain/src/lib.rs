//! Talking to a Monero node: checking it, fetching blocks quickly, decoys, the
//! fee rate, and instant updates over ZMQ.

pub mod decoys;
pub mod error;
pub mod fetch;
pub mod node;
pub mod send_context;
pub mod transport;
pub mod zmq;
mod zmtp;
