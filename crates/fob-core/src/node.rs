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

use crate::{
    catch_up::link_speed,
    fetch,
    network::Network,
    price::grouped,
    transport::{CertProblem, Transport},
};

/// How long the node has to answer `get_info` (and say its last block).
const ANSWER: Duration = Duration::from_secs(5);
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

impl Step {
    pub fn title(self) -> &'static str {
        match self {
            Step::Certificate => "Certificate",
            Step::Answers => "Answers",
            Step::Network => "Right network",
            Step::UpToDate => "Up to date",
            Step::Blocks => "Sends blocks to Fob",
        }
    }

    /// The verdict when this check is the first to fail.
    fn failed(self) -> &'static str {
        match self {
            Step::Certificate => "Certificate not accepted",
            Step::Answers => "Doesn't answer",
            Step::Network => "Wrong network",
            Step::UpToDate => "Not up to date",
            Step::Blocks => "Doesn't send blocks",
        }
    }
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

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub step: Step,
    /// "84 ms", "stagenet", "block 1,792,004 · 1 min ago", "2 days behind".
    pub result: String,
    pub mark: Mark,
}

impl Row {
    pub fn title(&self) -> &'static str {
        self.step.title()
    }

    /// For TalkBack: "Up to date: 2 days behind", "Answers: not checked".
    pub fn label(&self) -> String {
        match self.mark {
            Mark::Skipped => format!("{}: not checked", self.title()),
            _ => format!("{}: {}", self.title(), self.result),
        }
    }
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
            .map(|step| Row { step, result: "checking…".into(), mark: Mark::Checking })
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

    /// "Working", "Checking…", or the first failed check's words.
    pub fn verdict(&self) -> &'static str {
        if self.checking() {
            return "Checking…";
        }
        self.rows.iter().find(|r| r.mark == Mark::Fail).map_or("Working", |r| r.step.failed())
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

    /// One line for the list of other nodes: "120 ms · in sync · fastest",
    /// "Didn't answer in 5 s", "Mainnet node: this wallet is stagenet".
    pub fn line(&self, fastest: bool) -> String {
        if self.checking() {
            return "Checking…".into();
        }
        let ms = self.latency_ms.unwrap_or(0);
        let Some(failed) = self.rows.iter().find(|r| r.mark == Mark::Fail) else {
            return format!("{ms} ms · in sync{}", if fastest { " · fastest" } else { "" });
        };
        match failed.step {
            Step::Certificate => format!("Certificate {}", failed.result),
            Step::Answers => capitalized(&failed.result),
            Step::Network => match self.nettype.as_deref() {
                Some(n) if !n.is_empty() => format!("{} node: this wallet is {}", capitalized(n), self.network.name()),
                _ => format!("Unknown network: this wallet is {}", self.network.name()),
            },
            Step::UpToDate | Step::Blocks => format!("{ms} ms · {}", failed.result),
        }
    }

    fn set(&mut self, step: Step, mark: Mark, result: impl Into<String>) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.step == step) {
            row.mark = mark;
            row.result = result.into();
        }
    }

    /// Every check still waiting is skipped: one it needs failed.
    fn stop(mut self) -> Report {
        for row in self.rows.iter_mut().filter(|r| r.mark == Mark::Checking) {
            row.mark = Mark::Skipped;
            row.result.clear();
        }
        self
    }

    /// The node didn't answer: its certificate (if that's why) or Answers fails.
    fn unanswered(mut self, failure: Failure) -> Report {
        let step = if matches!(failure, Failure::Certificate(_)) { Step::Certificate } else { Step::Answers };
        self.set(step, Mark::Fail, failure.words(ANSWER));
        self.stop()
    }
}

/// "Didn't answer in 5 s" from "didn't answer in 5 s".
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Why a request to the node failed, in words for a check's row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Failure {
    Onion,
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
    /// "didn't answer in 5 s", "self-signed, not accepted".
    fn words(&self, limit: Duration) -> String {
        match self {
            Failure::Onion => "needs Tor, not in Fob yet".into(),
            Failure::BadAddress => "not an address Fob can read".into(),
            Failure::Certificate(problem) => format!("{}, not accepted", problem.words()),
            Failure::TimedOut => format!("didn't answer in {} s", limit.as_secs()),
            Failure::Refused => "refused to connect".into(),
            Failure::NotFound => "address not found".into(),
            Failure::NotMonero => "not a Monero node".into(),
            // monerod started with --rpc-login.
            Failure::Status(401) => "wants a login".into(),
            Failure::Status(code) => format!("answered {code}"),
            Failure::Unreadable => "sent blocks Fob can't read".into(),
            Failure::Other => "couldn't connect".into(),
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
    fn new(node: &Setting) -> Result<Live, Failure> {
        match reachable(&node.url) {
            Err(e) if e == NO_ONION => return Err(Failure::Onion),
            Err(_) => return Err(Failure::BadAddress),
            Ok(()) => {}
        }
        let transport = Transport::new(&normalize(&node.url), node.self_signed).map_err(|_| Failure::Other)?;
        Ok(Live { transport, self_signed: node.self_signed })
    }

    fn failure(&self, e: InterfaceError) -> Failure {
        if !self.self_signed
            && let Some(Err(problem)) = self.transport.certificate()
        {
            return Failure::Certificate(problem);
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

/// "just now", "1 min ago", "2 h ago".
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        _ => format!("{} h ago", seconds / 3600),
    }
}

/// "20 min behind", "5 h behind", "2 days behind".
fn behind(seconds: u64) -> String {
    match seconds {
        0..3600 => format!("{} min behind", (seconds / 60).max(1)),
        3600..172_800 => format!("{} h behind", seconds / 3600),
        _ => format!("{} days behind", seconds / 86_400),
    }
}

/// Up to date's mark and words, from the node's info and last block, the
/// tip this wallet last saw, and the time now (seconds since the epoch).
fn up_to_date(info: &Info, (last_height, timestamp): (u64, u64), known_tip: u64, now: u64) -> (Mark, String) {
    let blocks_behind = info.target_height.max(known_tip).saturating_sub(info.height);
    let age = now.saturating_sub(timestamp);
    if blocks_behind > BEHIND_BLOCKS || age > STALE {
        let stuck = if age > STALE { age } else { 0 };
        (Mark::Fail, behind((blocks_behind * BLOCK_SECONDS).max(stuck)))
    } else {
        (Mark::Pass, format!("block {} · {}", grouped(last_height), ago(age)))
    }
}

/// "31 blocks/s · 1.8 MB/s", or "0.8 blocks/s · 40 kB/s" for slow going.
fn speed(blocks: u64, bytes: u64, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64().max(0.001);
    let per_second = blocks as f64 / secs;
    let blocks = if per_second >= 9.95 { grouped(per_second.round() as u64) } else { format!("{per_second:.1}") };
    format!("{blocks} blocks/s · {}", link_speed(bytes as f64 / secs))
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
        Some(Err(problem)) => format!("{}, accepted", problem.words()),
        _ => "valid".into(),
    };
    report.set(Step::Certificate, Mark::Pass, certificate);
    report.set(Step::Answers, Mark::Pass, format!("{latency} ms"));
    report.latency_ms = Some(latency);
    report.nettype = Some(info.nettype.clone());
    progress(&report);

    let network = report.network.name();
    if info.nettype != network {
        let theirs = if info.nettype.is_empty() { "unknown network" } else { &info.nettype };
        report.set(Step::Network, Mark::Fail, format!("{theirs}, not {network}"));
        return report.stop();
    }
    report.set(Step::Network, Mark::Pass, network);
    progress(&report);

    // A node behind can still send blocks: both are checked.
    let (mark, words) = match tokio::time::timeout(ANSWER, probe.last_block()).await {
        Ok(Ok(last)) => up_to_date(&info, last, known_tip, now),
        Ok(Err(failure)) => (Mark::Fail, failure.words(ANSWER)),
        Err(_) => (Mark::Fail, Failure::TimedOut.words(ANSWER)),
    };
    report.set(Step::UpToDate, mark, words);
    progress(&report);

    // Recent blocks, short of the newest (which may not be served yet).
    let end = info.height.saturating_sub(2).max(1);
    let start = end.saturating_sub(SAMPLE - 1).max(1);
    let started = Instant::now();
    let (mark, words) = match tokio::time::timeout(BLOCKS, probe.blocks(start, end)).await {
        Ok(Ok((0, _))) => (Mark::Fail, "sent no blocks".into()),
        Ok(Ok((blocks, bytes))) => {
            let elapsed = started.elapsed();
            report.blocks_per_second = Some(blocks as f64 / elapsed.as_secs_f64().max(0.001));
            (Mark::Pass, speed(blocks, bytes, elapsed))
        }
        Ok(Err(failure)) => (Mark::Fail, failure.words(BLOCKS)),
        Err(_) => (Mark::Fail, format!("no blocks in {} s", BLOCKS.as_secs())),
    };
    report.set(Step::Blocks, mark, words);
    report
}

/// Checks `node` for a wallet on `network` that last saw `known_tip`, at
/// `now` (seconds since the epoch), telling `progress` as checks finish.
pub async fn check(network: Network, node: &Setting, known_tip: u64, now: u64, progress: &(dyn Fn(&Report) + Sync)) -> Report {
    let report = Report::pending(network, node);
    progress(&report);
    match Live::new(node) {
        Ok(live) => run(&live, report, known_tip, now, progress).await,
        Err(failure) => report.unanswered(failure),
    }
}

/// Checks several nodes at once, with each one's line for the list (the
/// fastest of two or more that pass says so).
pub async fn check_all(network: Network, nodes: &[Setting], known_tip: u64, now: u64) -> Vec<(Report, String)> {
    let handles: Vec<_> = nodes
        .iter()
        .map(|node| {
            let node = node.clone();
            tokio::spawn(async move { check(network, &node, known_tip, now, &|_| {}).await })
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
fn lines(reports: &[Report]) -> Vec<String> {
    let speed = |r: &Report| r.blocks_per_second.unwrap_or(0.0);
    let passing: Vec<usize> = (0..reports.len()).filter(|&i| reports[i].usable() == Usable::Use).collect();
    let fastest = if passing.len() > 1 { passing.iter().copied().max_by(|&a, &b| speed(&reports[a]).total_cmp(&speed(&reports[b]))) } else { None };
    reports.iter().enumerate().map(|(i, r)| r.line(Some(i) == fastest)).collect()
}

/// Whether `node` is on `network`, before switching to it.
pub async fn on_network(network: Network, node: &Setting) -> Result<(), String> {
    reachable(&node.url)?;
    let live = Live::new(node).map_err(|f| f.words(ANSWER))?;
    let info = live.info().await.map_err(|f| f.words(ANSWER))?;
    if info.nettype != network.name() {
        return Err(format!("that node is on {}, and this wallet is on {}", if info.nettype.is_empty() { "an unknown network" } else { &info.nettype }, network.name()));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
