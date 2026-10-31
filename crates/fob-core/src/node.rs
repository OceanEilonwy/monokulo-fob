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

/// What Fob says when asked for a node on an onion service. It has no Tor
/// yet, and looking the name up through the phone's DNS would tell whoever
/// answers which onion service this wallet uses.
pub const NO_ONION: &str = "Fob can't reach .onion nodes yet";

/// Whether `host` is an onion service's (`….onion`, any case, with or
/// without the root's dot).
pub fn is_onion(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    let n = host.len();
    host.eq_ignore_ascii_case("onion") || n > 6 && host.is_char_boundary(n - 6) && host[n - 6..].eq_ignore_ascii_case(".onion")
}

/// Refuses a node Fob can't reach without leaking it: one on an onion
/// service. Checked before anything connects (or looks a name up).
pub fn reachable(url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(&normalize(url)).map_err(|_| format!("{url} isn't an address Fob can read"))?;
    if is_onion(url.host_str().unwrap_or("")) { Err(NO_ONION.into()) } else { Ok(()) }
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
    reachable(url)?;
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
    if let Err(e) = reachable(&url) {
        return failed(url, e);
    }
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
    fn onion_hosts_are_known_by_name() {
        let onion = "http://xmrnodexxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.onion:18081";
        for host in ["abc.onion", "ABC.ONION", "abc.onion.", "a.b.onion", "onion"] {
            assert!(is_onion(host), "{host}");
        }
        for host in ["onion.example", "example.com", "notonion", "1.2.3.4", "", "é.onio"] {
            assert!(!is_onion(host), "{host}");
        }
        assert_eq!(reachable(onion), Err(NO_ONION.into()));
        assert_eq!(reachable("xmrnode.onion:18081"), Err(NO_ONION.into()), "without a scheme too");
        assert_eq!(reachable("https://user:pw@Node.Onion./"), Err(NO_ONION.into()));
        assert_eq!(reachable("node.example:18081"), Ok(()));
    }

    /// Refused before anything connects or looks the name up: a resolver
    /// would hear which onion service the wallet uses.
    #[tokio::test]
    async fn an_onion_node_is_refused_before_any_lookup() {
        let checked = check(Network::Stagenet, "xmrnode.onion:38081").await;
        assert_eq!((checked.reachable, checked.error.as_deref()), (false, Some(NO_ONION)));
        assert_eq!(on_network(Network::Stagenet, "http://xmrnode.onion:38081").await, Err(NO_ONION.into()));
        let err = Transport::new("http://xmrnode.onion:38081").err().expect("refused");
        assert!(format!("{err:?}").contains(NO_ONION), "{err:?}");
    }

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
