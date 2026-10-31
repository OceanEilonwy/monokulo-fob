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

/// How long one request may take, including reading a large batch of blocks.
const TIMEOUT: Duration = Duration::from_secs(60);

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
    pub fn new(url: &str) -> Result<Transport, InterfaceError> {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(90))
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
        let request = self.client.post(format!("{}/{route}", self.base)).body(body);
        let request = if route.ends_with(".bin") { request } else { request.header("Content-Type", "application/json") };
        let requests = self.requests.clone();
        async move {
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
            if let Ok(dir) = std::env::var("FOB_DUMP_BIN") {
                if route.ends_with(".bin") {
                    let n = requests.load(Ordering::Relaxed);
                    let _ = std::fs::write(format!("{dir}/{n}-{route}"), &body);
                }
            }
            Ok(body)
        }
    }
}
