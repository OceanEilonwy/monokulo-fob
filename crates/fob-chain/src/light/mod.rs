//! Syncing through a light wallet server: the MyMonero REST API that
//! monero-lws and monokulo-lws speak, with monero-lws's subaddress calls.
//! The server holds the wallet's view key and does the scanning; Fob asks
//! what the wallet received and which of its outputs may have been spent.
//!
//! - [`lws`]: the wire types, copied from monokulo-lws's `lws-api`.
//! - [`client`]: [`LightClient`], one method per call, over the node's own
//!   transport (certificate pinning, Basic auth, no redirects, no onion).
//! - [`proof`]: a server's proof that a transaction is in a block, checked.
//! - [`hashes`]: block hashes from a node, to hold those proofs to, asked
//!   over stretches of the chain so the node can't tell the wallet's blocks.

pub mod client;
pub mod hashes;
pub mod lws;
pub mod proof;

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
    /// On an onion service: no Tor yet (decision 73). An `.i2p` host too:
    /// there's no I2P either, and looking its name up would leak it.
    Onion,
    /// `http://` to a host that isn't this device: the view key would cross
    /// the network unencrypted. Only `https://` there.
    PlainHttp,
    /// Text that doesn't read as a server's address.
    BadAddress,
    /// The server is on another network: its name for it, and the wallet's.
    OtherNetwork { theirs: String, ours: String },
    /// The server has subaddresses off (`max_subaddresses` 0): it would miss
    /// every payment to a request or a share.
    NoSubaddresses,
    /// Outputs the server named that failed Fob's checks, and how many.
    BadOutputs(usize),
    /// An amount the server gave doesn't open the commitment it gave for
    /// that output: the server lied or is broken. Nothing of that answer
    /// was taken.
    AmountMismatch,
    /// The server's answer contradicts itself or what Fob already knows: one
    /// output named twice with different keys, one place on the chain given
    /// to two outputs, a block past the chain's tip. Nothing of it was taken.
    Contradiction,
    /// A transaction's proof that it's in a block doesn't hold, or the
    /// block it names isn't the one a node has at that height: the server
    /// named a payment the chain doesn't hold. Nothing of it was taken.
    NotOnChain,
    /// Light mode was asked for with no server chosen.
    NoServerSet,
}

/// A light wallet server's address as people type it: `host:port` gets
/// `https://` (`http://` for this device itself), no trailing slash. Unlike
/// a node, a server is asked with the view key in every request, so plain
/// HTTP is the exception, not the default.
pub fn normalize(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.contains("://") {
        return url.to_owned();
    }
    let host = reqwest::Url::parse(&format!("http://{url}")).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
    if is_loopback(&host) { format!("http://{url}") } else { format!("https://{url}") }
}

/// Refuses a server Fob mustn't send the view key to: an onion or I2P host
/// (no Tor or I2P, and the name lookup would leak it), plain `http://` to
/// anything but this device, or text that isn't an address.
pub fn allowed(url: &str) -> Result<(), Problem> {
    let parsed = reqwest::Url::parse(&normalize(url)).map_err(|_| Problem::BadAddress)?;
    let host = parsed.host_str().unwrap_or("").trim_end_matches('.');
    if host.is_empty() {
        return Err(Problem::BadAddress);
    }
    if crate::node::is_onion(host) || is_i2p(host) {
        return Err(Problem::Onion);
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(host) => Ok(()),
        "http" => Err(Problem::PlainHttp),
        _ => Err(Problem::BadAddress),
    }
}

/// `localhost`, 127.0.0.0/8 or `::1`: this device itself.
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.');
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn is_i2p(host: &str) -> bool {
    let n = host.len();
    host.eq_ignore_ascii_case("i2p") || n > 4 && host.is_char_boundary(n - 4) && host[n - 4..].eq_ignore_ascii_case(".i2p")
}

/// The network as the API names it in `/get_version`.
pub fn api_network(network: Network) -> &'static str {
    match network {
        Network::Mainnet => "main",
        Network::Stagenet => "stage",
    }
}
