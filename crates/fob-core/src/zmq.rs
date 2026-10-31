//! Instant updates: hearing of new blocks and pool transactions from the
//! node's ZMQ publisher (monerod's `--zmq-pub`) instead of waiting for the
//! next poll.
//!
//! An announcement only says "ask now". What the wallet finds still comes
//! from the node's RPC answers, exactly as when nothing announces anything,
//! so a lost, late, duplicated or forged announcement costs at most a poll
//! interval or one early request, never a wrong balance. Polling carries on
//! underneath, less often while the publisher is heard.
//!
//! The publisher must be on the node's own host: Fob never connects
//! anywhere the person didn't choose. Off unless turned on.
//!
//! Adapted from Monokulo's engine (`crates/engine/src/node_events.rs`):
//! the topics, reading an announcement, reconnecting with a growing pause.

use std::time::Duration;

use crate::network::Network;

/// monerod's topics for a transaction entering the pool and for a new
/// main-chain tip, in their small forms: only the topic is read.
pub(crate) const POOL_TOPIC: &str = "json-minimal-txpool_add";
pub(crate) const CHAIN_TOPIC: &str = "json-minimal-chain_main";

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(60);

/// How instant updates are doing, for Settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ZmqStatus {
    /// Turned off: polling alone.
    #[default]
    Off,
    /// Turned on, not connected yet (or not syncing just now).
    Connecting,
    /// Connected and subscribed.
    Listening,
    /// The last try failed, and why, in a few words. Retried with a
    /// growing pause, up to a minute.
    CantConnect(String),
}

impl ZmqStatus {
    /// The subtitle under "Instant updates (ZMQ)":
    /// "node.example:18083 · listening", or "Off".
    pub fn text(&self, endpoint: Option<&str>) -> String {
        let word = match self {
            ZmqStatus::Off => return "Off".to_owned(),
            ZmqStatus::Connecting => "connecting",
            ZmqStatus::Listening => "listening",
            ZmqStatus::CantConnect(_) => "can't connect",
        };
        let place = endpoint.map(|e| e.trim_start_matches("tcp://")).unwrap_or("");
        format!("{place} · {word}")
    }
}

/// The node's host as a URL has it (`[::1]` for IPv6), lowercased.
fn node_host(node: &str) -> Option<String> {
    let url = reqwest::Url::parse(&crate::node::normalize(node)).ok()?;
    url.host_str().filter(|h| !h.is_empty()).map(|h| h.to_ascii_lowercase())
}

/// The publisher to offer for `node`: monerod's usual ZMQ port on the same
/// host. Only a suggestion: nothing connects until it's turned on.
pub fn suggest(network: Network, node: &str) -> Option<String> {
    let port = match network {
        Network::Mainnet => 18083,
        Network::Stagenet => 38083,
    };
    Some(format!("tcp://{}:{port}", node_host(node)?))
}

/// Checks a publisher as typed (`tcp://host:port`, or `host:port`) for a
/// wallet using `node`, and writes it one way. Only TCP, and only the
/// node's own host.
pub fn endpoint(node: &str, typed: &str) -> Result<String, String> {
    let typed = typed.trim().trim_end_matches('/');
    if typed.is_empty() {
        return Err("enter the node's ZMQ address, like tcp://host:18083".into());
    }
    let with_scheme = if typed.contains("://") { typed.to_owned() } else { format!("tcp://{typed}") };
    let url = reqwest::Url::parse(&with_scheme).map_err(|_| format!("{typed} isn't an address Fob can read"))?;
    if url.scheme() != "tcp" {
        return Err("Fob listens over tcp:// only".into());
    }
    if !url.username().is_empty() || url.password().is_some() || !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return Err("only a host and port: tcp://host:18083".into());
    }
    let host = url.host_str().filter(|h| !h.is_empty()).ok_or("add the host: tcp://host:18083")?.to_ascii_lowercase();
    let port = url.port().filter(|p| *p != 0).ok_or("add the port: tcp://host:18083")?;
    let node_host = node_host(node).ok_or("set a node first")?;
    if host != node_host {
        return Err(format!("instant updates come from your node's own host, {node_host}"));
    }
    Ok(format!("tcp://{host}:{port}"))
}

#[derive(Debug, PartialEq)]
pub(crate) enum Announcement {
    Pool,
    Chain,
}

/// What one message announces. monerod sends each as one frame,
/// `<topic>:<json>`; the JSON is not read.
pub(crate) fn announcement(frame: &[u8]) -> Option<Announcement> {
    let end = frame.iter().position(|&b| b == b':')?;
    match &frame[..end] {
        topic if topic == POOL_TOPIC.as_bytes() => Some(Announcement::Pool),
        topic if topic == CHAIN_TOPIC.as_bytes() => Some(Announcement::Chain),
        _ => None,
    }
}

/// What the subscriber tells the wallet.
pub(crate) trait Events: Send + Sync {
    fn status(&self, status: ZmqStatus);
    /// The node has a new tip.
    fn chain(&self);
    /// A transaction entered the node's pool.
    fn pool(&self);
}

/// A failure in a few words, for Settings.
fn reason(error: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    match error.kind() {
        K::ConnectionRefused => "nothing listening on that port".into(),
        K::TimedOut => "no answer".into(),
        K::UnexpectedEof | K::ConnectionReset => "the node closed the connection".into(),
        K::NotFound => "host not found".into(),
        _ => error.to_string(),
    }
}

/// Listens to `endpoint` until aborted, connecting again (after a growing
/// pause) whenever it can't be reached or the connection fails.
pub(crate) async fn listen(endpoint: String, events: impl Events) {
    let mut retry = FIRST_RETRY;
    events.status(ZmqStatus::Connecting);
    loop {
        let error = session(&endpoint, &events, &mut retry).await;
        events.status(ZmqStatus::CantConnect(error));
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(LAST_RETRY);
    }
}

/// One connection: when it ends, for whatever reason, so does the session.
async fn session(endpoint: &str, events: &impl Events, retry: &mut Duration) -> String {
    let mut subscriber = match crate::zmtp::connect(endpoint, &[POOL_TOPIC, CHAIN_TOPIC]).await {
        Ok(subscriber) => subscriber,
        Err(error) => return reason(&error),
    };
    *retry = FIRST_RETRY;
    events.status(ZmqStatus::Listening);
    // Whatever was announced while nobody listened.
    events.chain();
    loop {
        let frame = match subscriber.recv().await {
            Ok(frame) => frame,
            Err(error) => return reason(&error),
        };
        match announcement(&frame) {
            Some(Announcement::Pool) => events.pool(),
            Some(Announcement::Chain) => events.chain(),
            None => {}
        }
    }
}

#[cfg(test)]
pub(crate) mod test_publisher;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use test_publisher::Publisher;

    #[test]
    fn the_suggestion_is_the_usual_port_on_the_node_s_host() {
        assert_eq!(suggest(Network::Mainnet, "https://Node.Example:18089").as_deref(), Some("tcp://node.example:18083"));
        assert_eq!(suggest(Network::Stagenet, "node.example:38081").as_deref(), Some("tcp://node.example:38083"));
        assert_eq!(suggest(Network::Stagenet, "http://user:pass@[::1]:38081").as_deref(), Some("tcp://[::1]:38083"));
    }

    #[test]
    fn endpoints_are_tcp_on_the_node_s_own_host() {
        let node = "https://node.example:18089";
        assert_eq!(endpoint(node, " tcp://node.example:18083/ ").unwrap(), "tcp://node.example:18083");
        assert_eq!(endpoint(node, "Node.Example:28083").unwrap(), "tcp://node.example:28083");
        assert_eq!(endpoint("http://[::1]:38081", "tcp://[::1]:38083").unwrap(), "tcp://[::1]:38083");
        assert_eq!(endpoint("http://127.0.0.1:38081", "127.0.0.1:38083").unwrap(), "tcp://127.0.0.1:38083");
        for (typed, says) in [
            ("", "enter"),
            ("tcp://elsewhere.example:18083", "own host, node.example"),
            ("tcp://node.example.evil:18083", "own host"),
            ("tcp://user@node.example:18083", "only a host and port"),
            ("tcp://node.example:18083/path", "only a host and port"),
            ("tcp://node.example", "add the port"),
            ("tcp://node.example:0", "add the port"),
            ("tcp://node.example:99999", "isn't an address"),
            ("ipc:///tmp/zmq.sock", "tcp:// only"),
            ("udp://node.example:18083", "tcp:// only"),
            ("http://node.example:18083", "tcp:// only"),
        ] {
            let error = endpoint(node, typed).unwrap_err();
            assert!(error.contains(says), "{typed}: {error}");
        }
    }

    #[test]
    fn the_subtitle_names_the_place_and_how_it_is_doing() {
        let at = Some("tcp://node.example:18083");
        assert_eq!(ZmqStatus::Off.text(at), "Off");
        assert_eq!(ZmqStatus::Listening.text(at), "node.example:18083 · listening");
        assert_eq!(ZmqStatus::CantConnect("no answer".into()).text(at), "node.example:18083 · can't connect");
    }

    #[test]
    fn only_the_two_topics_are_announcements() {
        let pool = format!(r#"{POOL_TOPIC}:[{{"id":"ab","blob_size":1,"weight":1,"fee":1}}]"#);
        let chain = format!(r#"{CHAIN_TOPIC}:{{"first_height":1,"first_prev_id":"ab","ids":["cd"]}}"#);
        assert_eq!(announcement(pool.as_bytes()), Some(Announcement::Pool));
        assert_eq!(announcement(chain.as_bytes()), Some(Announcement::Chain));
        for other in [
            "json-full-txpool_add:[]",
            "json-full-chain_main:[]",
            "json-full-miner_data:{}",
            "json-minimal-txpool_add",
            "json-minimal-txpool_addx:[]",
            "",
        ] {
            assert_eq!(announcement(other.as_bytes()), None, "{other}");
        }
    }

    /// Everything the subscriber said, in order.
    #[derive(Clone, Default)]
    struct Heard(Arc<Mutex<Vec<String>>>);

    impl Heard {
        fn said(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    impl Events for Heard {
        fn status(&self, status: ZmqStatus) {
            self.0.lock().unwrap().push(format!("{status:?}"));
        }
        fn chain(&self) {
            self.0.lock().unwrap().push("chain".into());
        }
        fn pool(&self) {
            self.0.lock().unwrap().push("pool".into());
        }
    }

    /// A publisher, as monerod would be: each topic reaches its own call,
    /// and losing it is said. A publisher drops what it sends before the
    /// subscription reaches it, so each is sent until it lands.
    #[tokio::test]
    async fn a_publisher_s_announcements_reach_the_wallet_and_losing_it_is_said() {
        let publisher = Publisher::bind("tcp://127.0.0.1:0").await;
        let heard = Heard::default();
        let listener = tokio::spawn(listen(publisher.endpoint().to_owned(), heard.clone()));
        for topic in [POOL_TOPIC, CHAIN_TOPIC] {
            let call = if topic == POOL_TOPIC { "pool" } else { "chain" };
            let before = heard.said().iter().filter(|s| *s == call).count();
            let mut landed = false;
            for _ in 0..100 {
                publisher.send(&format!("{topic}:[]"));
                tokio::time::sleep(Duration::from_millis(20)).await;
                if heard.said().iter().filter(|s| *s == call).count() > before {
                    landed = true;
                    break;
                }
            }
            assert!(landed, "{topic} never arrived: {:?}", heard.said());
        }
        // Connecting, then listening, then a wake for what was missed.
        assert_eq!(heard.said()[..3], ["Connecting", "Listening", "chain"]);
        drop(publisher);
        for _ in 0..100 {
            if heard.said().last().is_some_and(|s| s.starts_with("CantConnect")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(heard.said().last().unwrap().contains("closed the connection"), "{:?}", heard.said());
        listener.abort();
    }

    #[tokio::test]
    async fn nothing_listening_is_said_in_a_few_words() {
        let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("tcp://{}", unused.local_addr().unwrap());
        drop(unused);
        let heard = Heard::default();
        let listener = tokio::spawn(listen(endpoint, heard.clone()));
        for _ in 0..100 {
            if heard.said().len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        listener.abort();
        assert_eq!(heard.said()[..2], ["Connecting".to_owned(), r#"CantConnect("nothing listening on that port")"#.to_owned()]);
    }
}
