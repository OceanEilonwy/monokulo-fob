//! Checking a node before using it: is it reachable, on the right network,
//! synced, and how fast does it hand over blocks.

use std::time::{Duration, Instant};

use monero_daemon_rpc::HttpTransport;

use crate::{fetch, network::Network, transport::Transport};

/// Blocks fetched to measure a node: enough to fill the pipeline a few times.
const SAMPLE: u64 = 200;
/// How long a check may take before the node counts as too slow.
const LIMIT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq)]
pub struct NodeCheck {
    pub url: String,
    /// Answered at all.
    pub reachable: bool,
    /// On the wallet's network.
    pub right_network: bool,
    /// Caught up with the chain itself.
    pub synced: bool,
    pub height: u64,
    /// One small request there and back.
    pub latency_ms: u64,
    /// Catching-up speed, measured with the real fetcher. 0 if not measured.
    pub blocks_per_second: f64,
    pub error: Option<String>,
}

/// A node address as people type it: `host:port` gets `http://`, no trailing slash.
pub fn normalize(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.contains("://") { url.to_owned() } else { format!("http://{url}") }
}

fn failed(url: String, error: String) -> NodeCheck {
    NodeCheck { url, reachable: false, right_network: false, synced: false, height: 0, latency_ms: 0, blocks_per_second: 0.0, error: Some(error) }
}

/// The node's own idea of its network, height and sync state, and how long
/// it took to say.
async fn info(transport: &Transport) -> Result<(String, u64, bool, Duration), String> {
    let started = Instant::now();
    let body = transport.post("get_info", b"{}".to_vec(), Some(64 * 1024)).await.map_err(|e| format!("{e:?}"))?;
    let latency = started.elapsed();
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|_| "not a Monero node".to_owned())?;
    let nettype = v.get("nettype").and_then(|n| n.as_str()).unwrap_or("").to_owned();
    let height = v.get("height").and_then(|h| h.as_u64()).unwrap_or(0);
    let synced = v.get("synchronized").and_then(|s| s.as_bool()).unwrap_or(false) && !v.get("busy_syncing").and_then(|b| b.as_bool()).unwrap_or(false);
    Ok((nettype, height, synced, latency))
}

/// Whether `url` is a node on `network`, without measuring its speed.
pub async fn on_network(network: Network, url: &str) -> Result<(), String> {
    let transport = Transport::new(&normalize(url)).map_err(|e| format!("{e:?}"))?;
    let (nettype, ..) = info(&transport).await?;
    if nettype != network.name() {
        return Err(format!("that node is on {}, and this wallet is on {}", if nettype.is_empty() { "an unknown network" } else { &nettype }, network.name()));
    }
    Ok(())
}

/// Checks `url` for a wallet on `network`, and measures how fast it is.
pub async fn check(network: Network, url: &str) -> NodeCheck {
    let url = normalize(url);
    let transport = match Transport::new(&url) {
        Ok(t) => t,
        Err(e) => return failed(url, format!("{e:?}")),
    };
    let (nettype, height, synced, latency) = match tokio::time::timeout(LIMIT, info(&transport)).await {
        Ok(Ok(i)) => i,
        Ok(Err(e)) => return failed(url, e),
        Err(_) => return failed(url, "no answer in 20 s".into()),
    };
    let mut check = NodeCheck {
        url,
        reachable: true,
        right_network: nettype == network.name(),
        synced,
        height,
        latency_ms: latency.as_millis() as u64,
        blocks_per_second: 0.0,
        error: None,
    };
    if !check.right_network {
        check.error = Some(format!("on {nettype}, not {}", network.name()));
        return check;
    }
    if height <= SAMPLE + 1 {
        return check;
    }
    let started = Instant::now();
    match tokio::time::timeout(LIMIT, fetch::blocks(&transport, height - 1 - SAMPLE, height - 2)).await {
        Ok(Ok(blocks)) => check.blocks_per_second = blocks.len() as f64 / started.elapsed().as_secs_f64(),
        Ok(Err(e)) => check.error = Some(format!("{e:?}")),
        Err(_) => check.error = Some("too slow: 200 blocks took over 20 s".into()),
    }
    check
}

/// Checks several nodes at once.
pub async fn check_all(network: Network, urls: &[String]) -> Vec<NodeCheck> {
    let handles: Vec<_> = urls.iter().map(|u| tokio::spawn({
        let u = u.clone();
        async move { check(network, &u).await }
    })).collect();
    let mut out = Vec::with_capacity(handles.len());
    for (h, url) in handles.into_iter().zip(urls) {
        out.push(h.await.unwrap_or_else(|e| failed(normalize(url), e.to_string())));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_written_one_way() {
        assert_eq!(normalize(" node.example:18089/ "), "http://node.example:18089");
        assert_eq!(normalize("https://node.example:18089"), "https://node.example:18089");
    }

    #[tokio::test]
    #[ignore = "needs public nodes"]
    async fn checks_tell_fast_from_wrong_over_http() {
        let stagenet = check(Network::Stagenet, Network::Stagenet.default_nodes()[0]).await;
        println!("{stagenet:?}");
        assert!(stagenet.reachable && stagenet.right_network && stagenet.blocks_per_second > 0.0);
        let mainnet_node = check(Network::Stagenet, Network::Mainnet.default_nodes()[0]).await;
        assert!(mainnet_node.reachable && !mainnet_node.right_network);
        let nobody = check(Network::Stagenet, "http://127.0.0.1:1").await;
        assert!(!nobody.reachable);
    }
}
