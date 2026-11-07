//! Syncing through a light wallet server: the MyMonero REST API that
//! monero-lws and monokulo-lws speak, with monero-lws's subaddress calls.
//! The server holds the wallet's view key and does the scanning; Fob asks
//! what the wallet received and which of its outputs may have been spent.
//!
//! - [`lws`]: the wire types, copied from monokulo-lws's `lws-api`.
//! - [`client`]: [`LightClient`], one method per call, over the node's own
//!   transport (certificate pinning, Basic auth, no redirects, no onion).

pub mod client;
pub mod lws;

pub use client::{LightClient, Report, check};

use fob_monero::network::Network;

use crate::transport::CertProblem;

/// Where a wallet's sync comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Fob fetches blocks from its node and scans them: the usual way.
    Node,
    /// A light wallet server scans for it.
    Light { url: String, self_signed: bool },
}

/// A light wallet server, and whether its self-signed certificate is accepted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Setting {
    pub url: String,
    pub self_signed: bool,
}

/// What can go wrong with a light wallet server. `fob-words` says each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// 403: the server doesn't know the wallet, hasn't approved it yet, or
    /// the key doesn't fit the address.
    Forbidden,
    /// 501: the server takes no new wallets (or no imports).
    NotTaken,
    /// 409: subaddresses are off on the server, or past its limit. Its words.
    Conflict(String),
    /// 400: the server refused the request. Its words.
    BadRequest(String),
    /// 503: the server's own node failed it.
    Unavailable,
    /// Any other HTTP status.
    Status(u16),
    /// No answer: no connection, a timeout, a line dropped. The transport's
    /// words, never the request (which holds the view key).
    Unreachable(String),
    /// The certificate didn't verify, and the server isn't marked to accept that.
    Certificate(CertProblem),
    /// It answered, but not as the API does.
    Unreadable,
    /// On an onion service: no Tor yet (decision 73).
    Onion,
    /// Text that doesn't read as a server's address.
    BadAddress,
    /// The server is on another network: its name for it, and the wallet's.
    OtherNetwork { theirs: String, ours: String },
    /// The server has subaddresses off (`max_subaddresses` 0): it would miss
    /// every payment to a request or a share.
    NoSubaddresses,
    /// Outputs the server named that failed Fob's checks, and how many.
    BadOutputs(usize),
    /// Light mode was asked for with no server chosen.
    NoServerSet,
}

/// The network as the API names it in `/get_version`.
pub fn api_network(network: Network) -> &'static str {
    match network {
        Network::Mainnet => "main",
        Network::Stagenet => "stage",
    }
}
