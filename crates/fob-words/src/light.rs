//! Syncing through a light wallet server: Settings' words, the Light server
//! screen's, and what a server's problems are called.

use fob_chain::light::{Problem, Report};
use fob_monero::amount::grouped;
use fob_screens::catch_up::{Amounts, Light, SyncState};

use crate::{device, node::certificate};

/// The Node card's switch: "Sync through".
pub const SYNC_THROUGH: &str = "Sync through";
/// Its two choices.
pub const THROUGH_NODE: &str = "Node";
pub const THROUGH_SERVER: &str = "Light wallet server";
/// The Node row's subtitle while syncing through a server.
pub const NODE_UNUSED: &str = "used again when you switch back";
/// "Instant updates"' subtitle while syncing through a server.
pub const ZMQ_UNUSED: &str = "The server watches the chain for you";
/// The row and the screen.
pub const SERVER_TITLE: &str = "Light wallet server";
/// The screen's fields and buttons.
pub const URL_LABEL: &str = "Server address";
pub const URL_HINT: &str = "https://lws.example.com:8443";
pub const SELF_SIGNED: &str = "Accept its own certificate";
pub const TEST: &str = "Test";
pub const USE: &str = "Sync through this server";
/// Before Test has been run on what's typed.
pub const NOT_TESTED: &str = "Test the server first";
/// The row before any server is chosen.
pub const NONE_CHOSEN: &str = "None chosen";

/// Send, while syncing through a server.
pub const SEND_LATER: &str = "Sending through a server comes next";
/// The import row when the server has no scans from the start to offer.
pub const IMPORT_NOT_OFFERED: &str = "Not offered by this server";

/// On the screen, and before light mode turns on.
pub const PRIVACY: &str =
    "The server gets this wallet's view key. It will see every payment you receive, how much, and when you open Fob. It can't spend. Choose a server you run or trust.";

/// On turning light mode off: "Fob will scan on this phone again from where
/// the server left off. The server still has the view key until you tell it
/// to forget the wallet."
pub fn turning_off() -> String {
    format!(
        "Fob will scan on this {} again from where the server left off. The server still has the view key until you tell it to forget the wallet.",
        device().noun()
    )
}

/// What a server's problem is called: in a sentence after "The server", or
/// as an error on its own.
pub fn problem(problem: &Problem) -> String {
    match problem {
        Problem::Forbidden => "Waiting for the server to approve this wallet".into(),
        Problem::NotTaken => "This server doesn't take new wallets".into(),
        Problem::Conflict(said) if said.is_empty() => "This server won't watch this wallet's addresses".into(),
        Problem::Conflict(said) => format!("This server won't watch this wallet's addresses: {said}"),
        Problem::BadRequest(said) if said.is_empty() => "The server turned the request down".into(),
        Problem::BadRequest(said) => format!("The server turned the request down: {said}"),
        Problem::Unavailable => "The server's own node isn't answering it".into(),
        Problem::Status(code) => format!("The server answered {code}"),
        Problem::Unreachable(why) => format!("Can't reach the server: {why}"),
        Problem::Certificate(cert) => format!("The server's certificate is {}, not accepted", certificate(*cert)),
        Problem::Unreadable => "The server's answer isn't a light wallet server's".into(),
        Problem::Onion => "Fob can't reach .onion or .i2p servers yet".into(),
        Problem::PlainHttp => "A light wallet server needs https, as the view key goes with every request. Plain http is only for a server on this device".into(),
        Problem::BadAddress => "That isn't an address Fob can read".into(),
        Problem::OtherNetwork { theirs, ours } => format!("The server is on {}, and this wallet is on {}", network_name(theirs), network_name(ours)),
        Problem::NoSubaddresses => "This server doesn't watch subaddresses, so it would miss payments to requests and shares".into(),
        Problem::BadOutputs(1) => "The server named a payment that isn't this wallet's: it wasn't counted".into(),
        Problem::BadOutputs(n) => format!("The server named {n} payments that aren't this wallet's: they weren't counted"),
        Problem::AmountMismatch => "The light server reported an amount that doesn't match the chain. Nothing from its answer was taken".into(),
        Problem::Contradiction => "The light server's answer contradicts itself. Nothing from it was taken".into(),
        Problem::NotOnChain => NOT_ON_CHAIN.into(),
        Problem::NoServerSet => "Choose a light wallet server first".into(),
    }
}

/// "stagenet" for the API's "stage".
fn network_name(api: &str) -> String {
    match api {
        "main" => "mainnet".into(),
        "stage" => "stagenet".into(),
        "test" => "testnet".into(),
        "" => "an unnamed network".into(),
        other => other.into(),
    }
}

/// Settings' row while syncing through a server: "lws.example:8443 · synced",
/// "… · waiting for approval", "… · can't reach", "… · 12 behind".
pub fn line(host: &str, state: Option<SyncState>) -> String {
    let said = match state {
        Some(SyncState::ServerSynced) => "synced".to_owned(),
        Some(SyncState::ServerWaiting) => "waiting for approval".into(),
        Some(SyncState::ServerFailed { local: false }) => "can't reach".into(),
        Some(SyncState::ServerFailed { local: true }) => "can't save".into(),
        Some(SyncState::ServerRefused) => "answer refused".into(),
        Some(SyncState::ServerBehind { blocks }) => format!("{} behind", grouped(blocks)),
        Some(SyncState::ServerScanning { year }) => format!("scanning from {year}"),
        Some(_) => "connecting".into(),
        None => return host.to_owned(),
    };
    format!("{host} · {said}")
}

/// One line of the Test's report.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportRow {
    pub label: String,
    pub value: String,
    /// The line is why the server can't be used.
    pub bad: bool,
}

/// The Test's report, as the screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportView {
    /// "lws.example:8443".
    pub address: String,
    /// "Ready to use", or why not.
    pub verdict: String,
    pub usable: bool,
    pub rows: Vec<ReportRow>,
}

/// What Test found, in words.
pub fn report(report: &Report) -> ReportView {
    let refusal = report.refusal();
    let row = |label: &str, value: String, bad: bool| ReportRow { label: label.into(), value, bad };
    let rows = match &report.outcome {
        Err(_) => vec![],
        Ok(v) => {
            let wrong = matches!(refusal, Some(Problem::OtherNetwork { .. }));
            vec![
                row("Network", network_name(&v.network), wrong),
                row("Height", grouped(v.blockchain_height), false),
                row(
                    "Subaddresses",
                    if v.max_subaddresses == 0 { "off".into() } else { format!("up to {}", grouped(u64::from(v.max_subaddresses))) },
                    v.max_subaddresses == 0,
                ),
                // Whether it takes new wallets only shows by asking with the
                // view key, which isn't sent before the switch.
                row("New wallets", "asked when you switch".into(), false),
                row("Server", format!("{} · API {}", v.server_type, v.api), false),
            ]
            .into_iter()
            .chain(report.pin.map(|pin| row(KEY_LABEL, key(&pin), false)))
            .collect()
        }
    };
    ReportView {
        address: report.address.clone(),
        verdict: refusal.as_ref().map_or_else(|| "Ready to use".to_owned(), problem),
        usable: refusal.is_none(),
        rows,
    }
}

/// The row of the key a server whose own certificate is accepted is held to.
pub const KEY_LABEL: &str = "Certificate key";
/// Under it.
pub const KEY_NOTE: &str = "The SHA3-256 of the server's public key. Fob talks to this server only while it answers with this key.";
/// When amounts can't be checked.
pub const SERVERS_WORD: &str =
    "This server doesn't send the chain's commitments, so Fob can't check the amounts it reports against the chain. They're the server's word.";
/// When amounts are held to commitments but payments aren't proved.
pub const COMMITMENTS_ONLY: &str = "This server doesn't prove where its payments are on the chain, so Fob holds their amounts only to the commitments it sends. A server could make up a payment whose commitment fits.";
/// When proofs came but the node couldn't be asked.
pub const NODE_UNREACHED: &str = "Fob couldn't reach the node to check this server's proofs against the chain. Until it can, its payments are held only to the commitments the proofs carry. It tries again at the next poll.";
/// When proofs came but the node hasn't those blocks yet.
pub const NODE_BEHIND: &str = "The node hasn't got the blocks this server's proofs name yet. Until it has, its payments are held only to the commitments the proofs carry. It tries again at the next poll.";
/// When the node checking proofs is Fob's default, the wallet having none.
pub const DEFAULT_NODE: &str = "This wallet has no node of its own, so Fob checks the server's proofs with its default node for the network.";
/// A proof that doesn't hold, or names a block the node hasn't got.
pub const NOT_ON_CHAIN: &str = "The light server named a payment that doesn't match the chain. Nothing from its answer was taken";
/// The row of the node proofs are checked with.
pub const NODE_LABEL: &str = "Checked with";
/// A history row's fee when the server didn't give one: the amount then
/// includes it.
pub const FEE_NOT_KNOWN: &str = "Not known · counted in the amount";

/// A key's fingerprint in groups of four, so two can be compared by eye.
pub fn key(pin: &[u8; 32]) -> String {
    let hex: String = pin.iter().map(|b| format!("{b:02x}")).collect();
    hex.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ")
}

/// What the Light server screen says of the server in use, before any Test:
/// the key it's held to, what its amounts are worth, and its last answer
/// if that failed.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Standing {
    pub rows: Vec<ReportRow>,
    /// Sentences under the rows.
    pub notes: Vec<String>,
}

/// The server in use, as the Light server screen shows it: `pin` (for a
/// server whose own certificate is accepted), what its payments and amounts
/// are worth and the node that checks its proofs, from its last answer
/// (`light`), and why that failed, if it did.
pub fn standing(pin: Option<&[u8; 32]>, light: &Light, failed: Option<&str>) -> Standing {
    let row = |label: &str, value: String, bad: bool| ReportRow { label: label.into(), value, bad };
    let mut said = Standing::default();
    if let Some(pin) = pin {
        said.rows.push(row(KEY_LABEL, key(pin), false));
        said.notes.push(KEY_NOTE.into());
    }
    match light.amounts {
        Amounts::NotYet => {}
        Amounts::Chain => said.rows.push(row("Amounts", "match the chain".into(), false)),
        Amounts::ChainPending if light.proof_node_unreached => {
            said.rows.push(row("Amounts", "not checked yet: can't reach the node".into(), false));
            said.notes.push(NODE_UNREACHED.into());
        }
        Amounts::ChainPending => {
            said.rows.push(row("Amounts", "not checked yet: the node is behind".into(), false));
            said.notes.push(NODE_BEHIND.into());
        }
        Amounts::Checked => {
            said.rows.push(row("Amounts", "commitments only".into(), false));
            said.notes.push(COMMITMENTS_ONLY.into());
        }
        Amounts::ServersWord => {
            said.rows.push(row("Amounts", "the server's word".into(), false));
            said.notes.push(SERVERS_WORD.into());
        }
        Amounts::Mismatch => said.rows.push(row("Amounts", "don't match the chain".into(), true)),
    }
    if let Some(node) = &light.proof_node {
        let value = if light.proof_node_default { format!("{node} · Fob's default") } else { node.clone() };
        said.rows.push(row(NODE_LABEL, value, light.proof_node_unreached));
        if light.proof_node_default {
            said.notes.push(DEFAULT_NODE.into());
        }
    }
    if let Some(failed) = failed {
        said.rows.push(row("Last answer", failed.into(), true));
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_say_how_the_server_stands() {
        assert_eq!(line("lws.example:8443", Some(SyncState::ServerSynced)), "lws.example:8443 · synced");
        assert_eq!(line("lws.example:8443", Some(SyncState::ServerWaiting)), "lws.example:8443 · waiting for approval");
        assert_eq!(line("lws.example:8443", Some(SyncState::ServerFailed { local: false })), "lws.example:8443 · can't reach");
        assert_eq!(line("h", Some(SyncState::ServerBehind { blocks: 1200 })), "h · 1,200 behind");
        assert_eq!(line("h", None), "h");
    }

    /// The chip through a server: synced, behind, far behind by the year,
    /// waiting for approval, failing.
    #[test]
    fn the_chip_says_the_server() {
        use fob_screens::catch_up::{Light, SyncStatus, year_of};
        let said = |s: &SyncStatus| crate::catch_up::chip(s.chip()).text;
        let base = SyncStatus { connected: true, scanned: 2_250_000, tip: 2_250_000, light: Some(Light::default()), ..SyncStatus::default() };
        assert_eq!(said(&SyncStatus { synced: true, ..base.clone() }), "Synced · server");
        assert_eq!(said(&SyncStatus { scanned: 2_249_988, ..base.clone() }), "Server 12 behind");
        let far = SyncStatus { scanned: 1_000_000, light: Some(Light { scanning_from: Some(1_577_836_800), ..Light::default() }), ..base.clone() };
        assert_eq!(said(&far), "Server scanning from 2020");
        assert_eq!(said(&SyncStatus { light: Some(Light { waiting_approval: true, ..Light::default() }), ..base.clone() }), "Waiting for approval");
        assert_eq!(said(&SyncStatus { error: Some("x".into()), ..base.clone() }), "Can't reach server");
        assert_eq!(said(&SyncStatus { connected: false, ..base }), "Connecting");
        assert_eq!((year_of(0), year_of(951_782_400), year_of(1_793_406_347)), (1970, 2000, 2026));
    }

    /// The Light server screen's rows for the server in use: the key in
    /// groups of four, what amounts are worth, a failure.
    #[test]
    fn standing_says_the_key_and_the_amounts() {
        let light = |amounts| Light { amounts, ..Light::default() };
        assert_eq!(standing(None, &light(Amounts::NotYet), None), Standing::default());
        let said = standing(Some(&[0xab; 32]), &light(Amounts::ServersWord), Some("Can't reach the server: refused"));
        assert_eq!(said.rows[0].value, ["abab"; 16].join(" "));
        assert_eq!((said.rows[1].label.as_str(), said.rows[1].value.as_str(), said.rows[1].bad), ("Amounts", "the server's word", false));
        assert!(said.rows[2].bad);
        assert_eq!(said.notes, vec![KEY_NOTE.to_owned(), SERVERS_WORD.to_owned()]);
        assert!(standing(None, &light(Amounts::Mismatch), None).rows[0].bad);
        assert_eq!(problem(&Problem::AmountMismatch), "The light server reported an amount that doesn't match the chain. Nothing from its answer was taken");
        assert!(problem(&Problem::PlainHttp).starts_with("A light wallet server needs https"));
    }

    /// The three ways a server's payments stand: proved and matching a
    /// node's chain, held to commitments only, or its word; a proof the
    /// node couldn't check yet; and the node it's checked with.
    #[test]
    fn standing_says_how_payments_are_checked() {
        let value = |light: &Light| standing(None, light, None).rows.iter().find(|r| r.label == "Amounts").map(|r| r.value.clone()).unwrap();
        let node = |amounts, default, unreached| Light { amounts, proof_node: Some("stagenet.xmr-tw.org:38081".into()), proof_node_default: default, proof_node_unreached: unreached, ..Light::default() };
        assert_eq!(value(&node(Amounts::Chain, false, false)), "match the chain");
        assert_eq!(value(&node(Amounts::Checked, false, false)), "commitments only");
        assert_eq!(value(&node(Amounts::ServersWord, false, false)), "the server's word");
        let pending = standing(None, &node(Amounts::ChainPending, true, true), None);
        assert_eq!(pending.rows[0].value, "not checked yet: can't reach the node");
        assert_eq!((pending.rows[1].label.as_str(), pending.rows[1].value.as_str(), pending.rows[1].bad), (NODE_LABEL, "stagenet.xmr-tw.org:38081 · Fob's default", true));
        assert_eq!(pending.notes, vec![NODE_UNREACHED.to_owned(), DEFAULT_NODE.to_owned()]);
        assert_eq!(value(&node(Amounts::ChainPending, false, false)), "not checked yet: the node is behind");
        assert_eq!(problem(&Problem::NotOnChain), "The light server named a payment that doesn't match the chain. Nothing from its answer was taken");
    }

    #[test]
    fn problems_have_words() {
        assert_eq!(problem(&Problem::NotTaken), "This server doesn't take new wallets");
        assert_eq!(problem(&Problem::OtherNetwork { theirs: "main".into(), ours: "stage".into() }), "The server is on mainnet, and this wallet is on stagenet");
        assert!(turning_off().starts_with("Fob will scan on this phone again"));
    }
}
