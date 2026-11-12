//! Checking a node: the Node screen's checks, in order, each one stopping
//! the ones that need it.
//!
//! 1. **Certificate** (https only): the node's certificate verifies, or is
//!    self-signed and the node is marked to accept that.
//! 2. **Answers**: `get_info` within 5 s; how long it took.
//! 3. **Right network**: the node's network is the wallet's.
//! 4. **Up to date**: the node isn't behind the network (its own target, or
//!    the tip this wallet last saw) and its last block isn't old. A node
//!    that's only behind can still be used, "anyway".
//! 5. **Sends blocks to Fob**: a hundred recent blocks through the fetcher
//!    that syncs, measured in blocks and bytes a second.
//!
//! The checks run against a [`Probe`]: the node itself, or canned answers
//! in tests.

use std::{future::Future, time::Duration};

use monero_daemon_rpc::HttpTransport;
use monero_wallet::interface::InterfaceError;
use tokio::time::Instant;

pub use fob_monero::network::is_onion;
use fob_monero::network::Network;
use crate::error::EndpointProblem;
use crate::{fetch, transport::{CertProblem, Transport}};

/// How long the node has to answer `get_info` (and say its last block).
pub const ANSWER: Duration = Duration::from_secs(5);
/// How long the sample of blocks may take.
const BLOCKS: Duration = Duration::from_secs(10);
/// Blocks fetched to measure a node: a few requests' worth.
const SAMPLE: u64 = 100;
/// Blocks a node may be behind and still be up to date: about ten minutes,
/// for a block or two it hasn't heard of yet.
const BEHIND_BLOCKS: u64 = 5;
/// The last block's age past which a node counts as stuck: blocks come
/// every two minutes, and an hour without one is rare.
const STALE: u64 = 3600;
/// Monero's target block time.
const BLOCK_SECONDS: u64 = 120;

/// Marks an onion node's refusal in the interface error's text, which is only
/// a string: [`sync_failure`] reads it back, and `fob-words` says it. It has no
/// Tor yet, and looking the name up through the phone's DNS would tell whoever
/// answers which onion service this wallet uses.
pub const ONION_REFUSED: &str = "onion node refused";

/// Marks a refused plain-http login the same way [`ONION_REFUSED`] does.
pub const PLAIN_LOGIN_REFUSED: &str = "plain http login refused";

/// Refuses a node Fob can't reach without leaking it: one on an onion
/// service, or one whose login would cross the network in the clear (plain
/// http to anything but this device or a private network). `host:port`
/// means http, as most nodes serve, so a login there needs `https://`.
/// Checked before anything connects (or looks a name up).
pub fn reachable(url: &str) -> Result<(), EndpointProblem> {
    let parsed = reqwest::Url::parse(&normalize(url)).map_err(|_| EndpointProblem::Unreadable(url.to_owned()))?;
    let host = parsed.host_str().unwrap_or("");
    if is_onion(host) {
        return Err(EndpointProblem::Onion);
    }
    let login = !parsed.username().is_empty() || parsed.password().is_some();
    if login && parsed.scheme() == "http" && !is_local(host) {
        return Err(EndpointProblem::PlainLogin);
    }
    Ok(())
}

/// Whether `host` is this device or on a private network: where a login
/// over plain http stays off the internet.
fn is_local(host: &str) -> bool {
    use std::net::IpAddr;
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
        Err(_) => false,
    }
}

/// A node address as people type it: `host:port` gets `http://`, no trailing slash.
pub fn normalize(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.contains("://") { url.to_owned() } else { format!("http://{url}") }
}

/// A node as shown: host and port, no scheme, and never a login in the URL.
pub fn address(url: &str) -> String {
    match reqwest::Url::parse(&normalize(url)) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_owned(),
            _ => url.trim().to_owned(),
        },
        Err(_) => url.trim().to_owned(),
    }
}

/// A node and whether its self-signed certificate is accepted.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Setting {
    pub url: String,
    pub self_signed: bool,
}

/// One of the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Certificate,
    Answers,
    Network,
    UpToDate,
    Blocks,
}

/// How a check came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Pass,
    Fail,
    Checking,
    /// Not run: one it needs failed.
    Skipped,
}

/// What a check found.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Checking,
    /// Not run: one it needs failed.
    Skipped,
    /// The certificate's fine, or has this problem and was accepted anyway.
    Certificate(Option<CertProblem>),
    /// How long `get_info` took.
    Latency(u64),
    /// The node is on the wallet's network.
    Network(String),
    /// The node's network, empty when it didn't say, and the wallet's.
    WrongNetwork { theirs: String, ours: String },
    /// Its last block, and how many seconds ago that was.
    Block { height: u64, age: u64 },
    Behind { seconds: u64 },
    /// A request failed; `limit` is how many seconds it had.
    Failed { failure: Failure, limit: u64 },
    /// It answered with none.
    NoBlocks,
    /// None within the time allowed.
    NoBlocksWithin { seconds: u64 },
    Speed { blocks_per_second: f64, bytes_per_second: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub step: Step,
    pub outcome: Outcome,
    pub mark: Mark,
}

/// A node's verdict over all its checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Checking,
    Working,
    /// The first check to fail.
    Failed(Step),
}

/// One line for the list of other nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Checking,
    InSync { ms: u64, fastest: bool },
    /// Its certificate failed, with what's wrong with it.
    Certificate(Outcome),
    /// It didn't answer, with why.
    NoAnswer(Outcome),
    /// Its network, empty when it didn't say, and the wallet's.
    OtherNetwork { theirs: String, ours: String },
    /// It answers but is behind or won't send blocks.
    Poor { ms: u64, outcome: Outcome },
}

/// Why a node can't be switched to.
#[derive(Clone, Debug, PartialEq)]
pub enum SwitchRefused {
    Endpoint(EndpointProblem),
    Failed(Failure),
    /// Its network, empty when it didn't say, and the wallet's.
    OtherNetwork { theirs: String, ours: String },
}

/// Whether a checked node can be switched to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usable {
    Use,
    /// Only Up to date failed: "Use it anyway".
    UseAnyway,
    No,
}

/// A node's checks, as far as they've got.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub url: String,
    pub self_signed: bool,
    /// "node.example:38081".
    pub address: String,
    pub rows: Vec<Row>,
    network: Network,
    latency_ms: Option<u64>,
    /// The node's network, once it said.
    nettype: Option<String>,
    blocks_per_second: Option<f64>,
}

impl Report {
    /// Before any answer: every check "checking…", a Certificate first for https.
    pub fn pending(network: Network, node: &Setting) -> Report {
        let url = normalize(&node.url);
        let https = url.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://"));
        let steps = [Step::Certificate, Step::Answers, Step::Network, Step::UpToDate, Step::Blocks];
        let rows = steps
            .into_iter()
            .filter(|s| https || *s != Step::Certificate)
            .map(|step| Row { step, outcome: Outcome::Checking, mark: Mark::Checking })
            .collect();
        Report {
            address: address(&url),
            url,
            self_signed: node.self_signed,
            rows,
            network,
            latency_ms: None,
            nettype: None,
            blocks_per_second: None,
        }
    }

    pub fn checking(&self) -> bool {
        self.rows.iter().any(|r| r.mark == Mark::Checking)
    }

    /// Still checking, working, or the first check to fail.
    pub fn verdict(&self) -> Verdict {
        if self.checking() {
            return Verdict::Checking;
        }
        self.rows.iter().find(|r| r.mark == Mark::Fail).map_or(Verdict::Working, |r| Verdict::Failed(r.step))
    }

    /// The verdict's mark: Checking, Fail when any check failed, else Pass.
    pub fn mark(&self) -> Mark {
        match () {
            _ if self.checking() => Mark::Checking,
            _ if self.rows.iter().any(|r| r.mark == Mark::Fail) => Mark::Fail,
            _ => Mark::Pass,
        }
    }

    pub fn usable(&self) -> Usable {
        let mut failed = self.rows.iter().filter(|r| r.mark == Mark::Fail).map(|r| r.step).peekable();
        match () {
            _ if self.checking() => Usable::No,
            _ if failed.peek().is_none() => Usable::Use,
            _ if failed.all(|s| s == Step::UpToDate) => Usable::UseAnyway,
            _ => Usable::No,
        }
    }

    /// What this node's line in the list of other nodes says.
    pub fn line(&self, fastest: bool) -> Line {
        if self.checking() {
            return Line::Checking;
        }
        let ms = self.latency_ms.unwrap_or(0);
        let Some(failed) = self.rows.iter().find(|r| r.mark == Mark::Fail) else {
            return Line::InSync { ms, fastest };
        };
        match failed.step {
            Step::Certificate => Line::Certificate(failed.outcome.clone()),
            Step::Answers => Line::NoAnswer(failed.outcome.clone()),
            Step::Network => Line::OtherNetwork { theirs: self.nettype.clone().unwrap_or_default(), ours: self.network.name().to_owned() },
            Step::UpToDate | Step::Blocks => Line::Poor { ms, outcome: failed.outcome.clone() },
        }
    }

    fn set(&mut self, step: Step, mark: Mark, outcome: Outcome) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.step == step) {
            row.mark = mark;
            row.outcome = outcome;
        }
    }

    /// Every check still waiting is skipped: one it needs failed.
    fn stop(mut self) -> Report {
        for row in self.rows.iter_mut().filter(|r| r.mark == Mark::Checking) {
            row.mark = Mark::Skipped;
            row.outcome = Outcome::Skipped;
        }
        self
    }

    /// The node didn't answer: its certificate (if that's why) or Answers fails.
    fn unanswered(mut self, failure: Failure) -> Report {
        let step = if matches!(failure, Failure::Certificate(_)) { Step::Certificate } else { Step::Answers };
        self.set(step, Mark::Fail, Outcome::Failed { failure, limit: ANSWER.as_secs() });
        self.stop()
    }
}

/// Why a sync step failed, as the node checks would see it: from the error
/// and the certificate the connection's last handshake saw. The error is the
/// interface error's debug text, a string upstream, so it's read here and goes
/// no further than this enum.
pub fn sync_failure(text: &str, certificate: Option<Result<(), CertProblem>>, self_signed: bool) -> Failure {
    // Its variant first.
    match Failure::refused(certificate, self_signed) {
        Some(refused) => refused,
        None if text.contains(ONION_REFUSED) => Failure::Onion,
        None if text.contains(PLAIN_LOGIN_REFUSED) => Failure::PlainLogin,
        None if text.starts_with("InvalidInterface(") || text.starts_with("scanning") => Failure::Unreadable,
        None => Failure::from_text(text),
    }
}

/// Why a request to the node failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Onion,
    /// A login that would go over plain http.
    PlainLogin,
    BadAddress,
    /// The certificate didn't verify (and the node isn't marked to accept that).
    Certificate(CertProblem),
    TimedOut,
    Refused,
    NotFound,
    /// It answered, but not as a Monero node does.
    NotMonero,
    /// An HTTP error status.
    Status(u16),
    /// Blocks that don't read as blocks.
    Unreadable,
    Other,
}

impl Failure {
    /// The node's certificate, if that's why a request failed: one with a
    /// problem the node isn't marked to accept, or one no mark accepts.
    fn refused(certificate: Option<Result<(), CertProblem>>, self_signed: bool) -> Option<Failure> {
        match certificate {
            Some(Err(problem)) if !self_signed || !problem.acceptable() => Some(Failure::Certificate(problem)),
            _ => None,
        }
    }

    /// From the transport's error text (reqwest's, with its causes).
    fn from_text(text: &str) -> Failure {
        let lower = text.to_ascii_lowercase();
        if let Some(code) = lower.split("node answered ").nth(1).and_then(|s| s.get(..3)).and_then(|s| s.parse().ok()) {
            return Failure::Status(code);
        }
        match () {
            _ if lower.contains("connection refused") => Failure::Refused,
            _ if lower.contains("dns error") || lower.contains("failed to lookup address") => Failure::NotFound,
            _ if lower.contains("timed out") => Failure::TimedOut,
            _ => Failure::Other,
        }
    }
}

/// What `get_info` says.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Info {
    pub nettype: String,
    pub height: u64,
    /// The height the node is catching up to; 0 when it isn't.
    pub target_height: u64,
}

/// `get_info`'s answer. Older nodes say their network only as flags.
pub(crate) fn parse_info(body: &[u8]) -> Result<Info, Failure> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|_| Failure::NotMonero)?;
    let height = v.get("height").and_then(|h| h.as_u64()).ok_or(Failure::NotMonero)?;
    let flag = |name: &str| v.get(name).and_then(|f| f.as_bool()).unwrap_or(false);
    let nettype = match v.get("nettype").and_then(|n| n.as_str()) {
        Some(n) => n.to_owned(),
        None if flag("mainnet") => "mainnet".into(),
        None if flag("stagenet") => "stagenet".into(),
        None if flag("testnet") => "testnet".into(),
        None => String::new(),
    };
    Ok(Info { nettype, height, target_height: v.get("target_height").and_then(|t| t.as_u64()).unwrap_or(0) })
}

/// `get_last_block_header`'s answer: the block's height and when it was mined.
pub(crate) fn parse_last_block(body: &[u8]) -> Result<(u64, u64), Failure> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|_| Failure::NotMonero)?;
    let header = v.get("result").and_then(|r| r.get("block_header")).ok_or(Failure::NotMonero)?;
    let field = |name: &str| header.get(name).and_then(|n| n.as_u64()).ok_or(Failure::NotMonero);
    Ok((field("height")?, field("timestamp")?))
}

/// What the checks ask a node.
pub(crate) trait Probe: Sync {
    fn info(&self) -> impl Future<Output = Result<Info, Failure>> + Send;
    /// The last block's height and timestamp.
    fn last_block(&self) -> impl Future<Output = Result<(u64, u64), Failure>> + Send;
    /// Fetches `start..=end`: how many blocks came, and their bytes.
    fn blocks(&self, start: u64, end: u64) -> impl Future<Output = Result<(u64, u64), Failure>> + Send;
    /// The certificate at the last handshake: verified, or what was accepted
    /// anyway. `None` over plain HTTP.
    fn certificate(&self) -> Option<Result<(), CertProblem>>;
}

/// The node itself.
struct Live {
    transport: Transport,
    self_signed: bool,
}

impl Live {
    /// `pin`, the key the wallet holds this node to, if it holds it to one.
    fn new(node: &Setting, pin: Option<[u8; 32]>) -> Result<Live, Failure> {
        match reachable(&node.url) {
            Err(EndpointProblem::Onion) => return Err(Failure::Onion),
            Err(EndpointProblem::PlainLogin) => return Err(Failure::PlainLogin),
            Err(_) => return Err(Failure::BadAddress),
            Ok(()) => {}
        }
        let transport = Transport::pinned(&normalize(&node.url), node.self_signed, pin).map_err(|_| Failure::Other)?;
        Ok(Live { transport, self_signed: node.self_signed })
    }

    fn failure(&self, e: InterfaceError) -> Failure {
        if let Some(refused) = Failure::refused(self.transport.certificate(), self.self_signed) {
            return refused;
        }
        match e {
            InterfaceError::InterfaceError(text) => Failure::from_text(&text),
            InterfaceError::InvalidInterface(_) => Failure::Unreadable,
            _ => Failure::Other,
        }
    }
}

impl Probe for Live {
    async fn info(&self) -> Result<Info, Failure> {
        let body = self.transport.post("get_info", b"{}".to_vec(), Some(64 * 1024)).await.map_err(|e| self.failure(e))?;
        parse_info(&body)
    }

    async fn last_block(&self) -> Result<(u64, u64), Failure> {
        let request = br#"{"jsonrpc":"2.0","id":"0","method":"get_last_block_header"}"#.to_vec();
        let body = self.transport.post("json_rpc", request, Some(64 * 1024)).await.map_err(|e| self.failure(e))?;
        parse_last_block(&body)
    }

    async fn blocks(&self, start: u64, end: u64) -> Result<(u64, u64), Failure> {
        let before = self.transport.received();
        let blocks = fetch::blocks(&self.transport, start, end).await.map_err(|e| self.failure(e))?;
        Ok((blocks.len() as u64, self.transport.received() - before))
    }

    fn certificate(&self) -> Option<Result<(), CertProblem>> {
        self.transport.certificate()
    }
}

/// Up to date's mark and outcome, from the node's info and last block, the
/// tip this wallet last saw, and the time now (seconds since the epoch).
fn up_to_date(info: &Info, (last_height, timestamp): (u64, u64), known_tip: u64, now: u64) -> (Mark, Outcome) {
    let blocks_behind = info.target_height.max(known_tip).saturating_sub(info.height);
    let age = now.saturating_sub(timestamp);
    if blocks_behind > BEHIND_BLOCKS || age > STALE {
        let stuck = if age > STALE { age } else { 0 };
        (Mark::Fail, Outcome::Behind { seconds: (blocks_behind * BLOCK_SECONDS).max(stuck) })
    } else {
        (Mark::Pass, Outcome::Block { height: last_height, age })
    }
}

/// Runs the checks on `probe`, telling `progress` after each one.
pub(crate) async fn run<P: Probe>(probe: &P, mut report: Report, known_tip: u64, now: u64, progress: &(dyn Fn(&Report) + Sync)) -> Report {
    let started = Instant::now();
    let info = match tokio::time::timeout(ANSWER, probe.info()).await {
        Ok(Ok(info)) => info,
        Ok(Err(failure)) => return report.unanswered(failure),
        Err(_) => return report.unanswered(Failure::TimedOut),
    };
    let latency = started.elapsed().as_millis() as u64;
    let certificate = match probe.certificate() {
        Some(Err(problem)) => Some(problem),
        _ => None,
    };
    report.set(Step::Certificate, Mark::Pass, Outcome::Certificate(certificate));
    report.set(Step::Answers, Mark::Pass, Outcome::Latency(latency));
    report.latency_ms = Some(latency);
    report.nettype = Some(info.nettype.clone());
    progress(&report);

    let network = report.network.name();
    if info.nettype != network {
        report.set(Step::Network, Mark::Fail, Outcome::WrongNetwork { theirs: info.nettype.clone(), ours: network.to_owned() });
        return report.stop();
    }
    report.set(Step::Network, Mark::Pass, Outcome::Network(network.to_owned()));
    progress(&report);

    // A node behind can still send blocks: both are checked.
    let unanswered = |failure| Outcome::Failed { failure, limit: ANSWER.as_secs() };
    let (mark, outcome) = match tokio::time::timeout(ANSWER, probe.last_block()).await {
        Ok(Ok(last)) => up_to_date(&info, last, known_tip, now),
        Ok(Err(failure)) => (Mark::Fail, unanswered(failure)),
        Err(_) => (Mark::Fail, unanswered(Failure::TimedOut)),
    };
    report.set(Step::UpToDate, mark, outcome);
    progress(&report);

    // Recent blocks, short of the newest (which may not be served yet).
    let end = info.height.saturating_sub(2).max(1);
    let start = end.saturating_sub(SAMPLE - 1).max(1);
    let started = Instant::now();
    let (mark, outcome) = match tokio::time::timeout(BLOCKS, probe.blocks(start, end)).await {
        Ok(Ok((0, _))) => (Mark::Fail, Outcome::NoBlocks),
        Ok(Ok((blocks, bytes))) => {
            let secs = started.elapsed().as_secs_f64().max(0.001);
            let blocks_per_second = blocks as f64 / secs;
            report.blocks_per_second = Some(blocks_per_second);
            (Mark::Pass, Outcome::Speed { blocks_per_second, bytes_per_second: bytes as f64 / secs })
        }
        Ok(Err(failure)) => (Mark::Fail, Outcome::Failed { failure, limit: BLOCKS.as_secs() }),
        Err(_) => (Mark::Fail, Outcome::NoBlocksWithin { seconds: BLOCKS.as_secs() }),
    };
    report.set(Step::Blocks, mark, outcome);
    report
}

/// Checks `node` for a wallet on `network` that last saw `known_tip`, at
/// `now` (seconds since the epoch), telling `progress` as checks finish.
/// `pin` is the key the wallet holds the node to, if any: a check that
/// accepted another would call working a node the wallet itself refuses.
pub async fn check(network: Network, node: &Setting, pin: Option<[u8; 32]>, known_tip: u64, now: u64, progress: &(dyn Fn(&Report) + Sync)) -> Report {
    let report = Report::pending(network, node);
    progress(&report);
    match Live::new(node, pin) {
        Ok(live) => run(&live, report, known_tip, now, progress).await,
        Err(failure) => report.unanswered(failure),
    }
}

/// Checks several nodes at once, with each one's line for the list (the
/// fastest of two or more that pass says so). `pinned` is a node the wallet
/// holds to a key, with the key: that node, if it's among them, is checked
/// held to it, as in [`check`].
pub async fn check_all(network: Network, nodes: &[Setting], pinned: Option<&(Setting, [u8; 32])>, known_tip: u64, now: u64) -> Vec<(Report, Line)> {
    let handles: Vec<_> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            let pin = pinned.filter(|(setting, _)| normalize(&setting.url) == normalize(&node.url) && setting.self_signed == node.self_signed).map(|(_, pin)| *pin);
            tokio::spawn(async move { check(network, &node, pin, known_tip, now, &|_| {}).await })
        })
        .collect();
    let mut reports = Vec::with_capacity(handles.len());
    for (handle, node) in handles.into_iter().zip(nodes) {
        reports.push(handle.await.unwrap_or_else(|_| Report::pending(network, node).unanswered(Failure::Other)));
    }
    let lines = lines(&reports);
    reports.into_iter().zip(lines).collect()
}

/// Each report's line, "fastest" on the quickest at sending blocks when
/// two or more pass.
fn lines(reports: &[Report]) -> Vec<Line> {
    let speed = |r: &Report| r.blocks_per_second.unwrap_or(0.0);
    let passing: Vec<usize> = (0..reports.len()).filter(|&i| reports[i].usable() == Usable::Use).collect();
    let fastest = if passing.len() > 1 { passing.iter().copied().max_by(|&a, &b| speed(&reports[a]).total_cmp(&speed(&reports[b]))) } else { None };
    reports.iter().enumerate().map(|(i, r)| r.line(Some(i) == fastest)).collect()
}

/// Whether `node` is on `network`, before switching to it. Held to no pin:
/// switching to a node, the one in use included, is how a person accepts a
/// key, so the wallet forgets the old pin and pins the key it next meets.
pub async fn on_network(network: Network, node: &Setting) -> Result<(), SwitchRefused> {
    reachable(&node.url).map_err(SwitchRefused::Endpoint)?;
    let live = Live::new(node, None).map_err(SwitchRefused::Failed)?;
    let info = live.info().await.map_err(SwitchRefused::Failed)?;
    if info.nettype != network.name() {
        return Err(SwitchRefused::OtherNetwork { theirs: info.nettype, ours: network.name().to_owned() });
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
pub mod canned;
#[cfg(test)]
mod tests;
