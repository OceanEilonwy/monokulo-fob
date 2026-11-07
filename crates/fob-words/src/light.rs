//! Syncing through a light wallet server: Settings' words, the Light server
//! screen's, and what a server's problems are called.

use fob_chain::light::{Problem, Report};
use fob_monero::amount::grouped;
use fob_screens::catch_up::SyncState;

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
        Problem::Onion => "Fob can't reach .onion servers yet".into(),
        Problem::BadAddress => "That isn't an address Fob can read".into(),
        Problem::OtherNetwork { theirs, ours } => format!("The server is on {}, and this wallet is on {}", network_name(theirs), network_name(ours)),
        Problem::NoSubaddresses => "This server doesn't watch subaddresses, so it would miss payments to requests and shares".into(),
        Problem::BadOutputs(1) => "The server named a payment that isn't this wallet's: it wasn't counted".into(),
        Problem::BadOutputs(n) => format!("The server named {n} payments that aren't this wallet's: they weren't counted"),
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
        }
    };
    ReportView {
        address: report.address.clone(),
        verdict: refusal.as_ref().map_or_else(|| "Ready to use".to_owned(), problem),
        usable: refusal.is_none(),
        rows,
    }
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

    #[test]
    fn problems_have_words() {
        assert_eq!(problem(&Problem::NotTaken), "This server doesn't take new wallets");
        assert_eq!(problem(&Problem::OtherNetwork { theirs: "main".into(), ours: "stage".into() }), "The server is on mainnet, and this wallet is on stagenet");
        assert!(turning_off().starts_with("Fob will scan on this phone again"));
    }
}
