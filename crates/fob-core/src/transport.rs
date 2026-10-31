//! How Fob talks to a node: pooled keep-alive HTTP(S) connections, with
//! requests running side by side.
//!
//! monero-oxide's own `simple-request` transport opens a new connection for
//! every request and lets only one run at a time, which makes catching up
//! about one block a second. Here every request gets a pooled connection of
//! its own, so the fetcher can keep several batches in flight.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use monero_daemon_rpc::{HttpTransport, MoneroDaemon};
use monero_wallet::interface::InterfaceError;

/// How long a node may go quiet mid-request. There's no limit on a whole
/// request: decoy selection downloads the chain's output distribution,
/// megabytes from a slow public node.
const QUIET: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Transport {
    client: reqwest::Client,
    base: Arc<str>,
    requests: Arc<AtomicU64>,
}

pub type Daemon = MoneroDaemon<Transport>;

impl Transport {
    /// A daemon at `url` (`http://host:port` or `https://…`; user:pass@ in
    /// the URL is sent as Basic auth).
    pub async fn connect(url: &str) -> Result<Daemon, InterfaceError> {
        MoneroDaemon::new(Transport::new(url)?).await
    }

    /// The transport alone, to keep a handle on it (see [`requests`](Self::requests)).
    /// An onion node is refused here too, whatever path led to it (an
    /// older wallet file, say): there's no Tor to reach it through.
    pub fn new(url: &str) -> Result<Transport, InterfaceError> {
        crate::node::reachable(url).map_err(InterfaceError::InternalError)?;
        let client = reqwest::Client::builder()
            .read_timeout(QUIET)
            .connect_timeout(Duration::from_secs(15))
            // monerod closes idle keep-alive connections; reusing one it has
            // dropped fails the request, so idle ones go after a few seconds.
            .pool_idle_timeout(Duration::from_secs(8))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .map_err(|e| InterfaceError::InternalError(format!("HTTP client: {e}")))?;
        Ok(Transport { client, base: url.trim_end_matches('/').into(), requests: Arc::new(AtomicU64::new(0)) })
    }

    /// Requests made so far, for tests and benchmarks.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }
}

impl HttpTransport for Transport {
    fn post(
        &self,
        route: &str,
        body: Vec<u8>,
        response_size_limit: Option<usize>,
    ) -> impl Send + Future<Output = Result<Vec<u8>, InterfaceError>> {
        let url = format!("{}/{route}", self.base);
        let json = !route.ends_with(".bin");
        let client = self.client.clone();
        let requests = self.requests.clone();
        async move {
            // A pooled connection the node closed while idle fails before the
            // node sees the request; Monero's RPC calls are safe to repeat
            // (re-relaying a transaction included), so send it again.
            let mut attempt = 0;
            loop {
                let request = client.post(&url).body(body.clone());
                let request = if json { request.header("Content-Type", "application/json") } else { request };
                match Self::send(request, &requests, response_size_limit).await {
                    Err(InterfaceError::InterfaceError(e)) if attempt < 2 && crate::fetch::stale(&e) => attempt += 1,
                    other => return other,
                }
            }
        }
    }
}

impl Transport {
    async fn send(request: reqwest::RequestBuilder, requests: &AtomicU64, response_size_limit: Option<usize>) -> Result<Vec<u8>, InterfaceError> {
        requests.fetch_add(1, Ordering::Relaxed);
        let err = |e: reqwest::Error| {
            // reqwest's own message is only "error sending request"; the cause is below it.
            let e = e.without_url();
            let mut text = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(cause) = source {
                text.push_str(": ");
                text.push_str(&cause.to_string());
                source = cause.source();
            }
            InterfaceError::InterfaceError(text)
        };
        let mut response = request.send().await.map_err(err)?;
        if !response.status().is_success() {
            return Err(InterfaceError::InterfaceError(format!("node answered {}", response.status())));
        }
        // Read it all before the connection goes back to the pool.
        let mut body = Vec::with_capacity(response.content_length().unwrap_or(1024) as usize);
        while let Some(chunk) = response.chunk().await.map_err(err)? {
            body.extend_from_slice(&chunk);
            if response_size_limit.is_some_and(|limit| body.len() > limit) {
                return Err(InterfaceError::InvalidInterface("node's response was too large".into()));
            }
        }
        Ok(body)
    }
}
