//! Manual Sync: a wallet that only scans when asked, and what asking can mean.
//!
//! With it on, nothing looks for blocks by itself. Sync opens the Scan drawer,
//! whose choices are here. Pulling down on Home does what [`PullMode`] says.
//! Coins become spendable at [`ManualSettings::settled_confirmations`].

use serde::{Deserialize, Serialize};

/// The fewest confirmations a coin can be spent at, whatever the setting says:
/// the chain's own lock.
pub const SETTLED_MIN: u32 = 10;
/// The most Settled Confirmations can be: about a day.
pub const SETTLED_MAX: u32 = 720;
/// The most Incoming Scan Blocks can be: about a day.
pub const INCOMING_MAX: u32 = 720;
/// How far back a reorganisation is always followed.
const REORG_DEPTH: usize = 100;
/// Hashes kept past the settled point, so a payment just settled can still be checked.
const REORG_MARGIN: usize = 30;

pub(crate) const BLOCKS_A_DAY: u64 = 720;
/// About what a phone scans whole blocks at, fetching included.
const BLOCKS_A_SECOND: u64 = 200;

/// What pulling down on Home does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullMode {
    /// Nothing, and there's no gesture.
    Disabled,
    /// Updates the payments this wallet sent, and their change.
    Send,
    /// That, and looks for new money in the pool and the newest blocks.
    #[default]
    Auto,
}

/// A wallet's Manual Sync settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualSettings {
    pub on: bool,
    pub pull_mode: PullMode,
    pub incoming_scan_blocks: u32,
    pub settled_confirmations: u32,
}

/// What the Scan drawer can be asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanRequest {
    /// Payments that aren't settled yet, as a pull would.
    Unsettled,
    /// From the last scan to the newest block.
    FromLast,
    /// The newest `n` blocks.
    Back(u64),
    /// From the wallet's first block.
    Whole,
    /// From a block to the newest.
    Height(u64),
    /// Between two times, seconds since the epoch; no end means now.
    Dates { from: u64, to: Option<u64> },
}

/// How a scan of blocks gets done.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScanPlan {
    /// Forget from this block on and scan to the newest: Last scan moves.
    Rewind(u64),
    /// Scan just these, keeping everything else: Last scan stays.
    Window(u64, u64),
}

/// Blocks that end at the newest and reach Last scan can replace what's known.
/// Anything else leaves a gap, so it's only added to.
pub(crate) fn scan_plan(from: u64, to: u64, next_block: u64, tip: u64) -> ScanPlan {
    let newest = tip.saturating_sub(1);
    if to >= newest && from <= next_block { ScanPlan::Rewind(from) } else { ScanPlan::Window(from, to.min(newest)) }
}

/// One of the Scan drawer's quick choices, every word ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanOption {
    pub request: ScanRequest,
    pub label: String,
    pub note: String,
    pub cost: String,
    pub action: String,
}

/// The Block tab's facts about a typed block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    /// "~30 Oct 2026", when it can be told.
    pub mined: Option<String>,
    pub newest: String,
    pub first: String,
}

/// How long `blocks` take to scan: "~7 s", "~2 min".
pub fn estimate(blocks: u64) -> String {
    let seconds = (blocks / BLOCKS_A_SECOND).max(1);
    if seconds < 60 { format!("~{seconds} s") } else { format!("~{} min", (seconds + 30) / 60) }
}

/// "21,600".
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn payments(n: usize) -> String {
    if n == 1 { "1 payment".into() } else { format!("{n} payments") }
}

/// The drawer's quick choices.
pub(crate) fn scan_options(settings: ManualSettings, unsettled: usize, next_block: u64, first_block: u64, tip: u64) -> Vec<ScanOption> {
    let unsettled_note = match settings.pull_mode {
        PullMode::Auto => format!("{} · Pool · Last {} blocks", payments(unsettled), settings.incoming_scan_blocks),
        _ => payments(unsettled),
    };
    let blocks_option = |request, label: &str, note: String, blocks: u64| ScanOption {
        request,
        label: label.into(),
        note,
        cost: estimate(blocks),
        action: format!("Scan {} blocks", grouped(blocks)),
    };
    let behind = tip.saturating_sub(next_block);
    let whole = tip.saturating_sub(first_block);
    // Nothing before the wallet's first block is looked at.
    let back = |request, label: &str, blocks: u64| {
        let blocks = blocks.min(whole);
        blocks_option(request, label, format!("~{} blocks", grouped(blocks)), blocks)
    };
    vec![
        ScanOption { request: ScanRequest::Unsettled, label: "Unsettled payments".into(), note: unsettled_note, cost: "~1 s".into(), action: "Check payments".into() },
        blocks_option(ScanRequest::FromLast, "From last scan", format!("{} blocks", grouped(behind)), behind),
        back(ScanRequest::Back(BLOCKS_A_DAY), "Today", BLOCKS_A_DAY),
        back(ScanRequest::Back(BLOCKS_A_DAY * 7), "Past week", BLOCKS_A_DAY * 7),
        back(ScanRequest::Back(BLOCKS_A_DAY * 30), "Past month", BLOCKS_A_DAY * 30),
        blocks_option(ScanRequest::Whole, "The whole wallet", format!("From block {} · ~{} blocks", grouped(first_block), grouped(whole)), whole),
    ]
}

/// How many block hashes to keep for finding reorganisations.
pub(crate) fn keep_hashes(settled_confirmations: u32) -> usize {
    REORG_DEPTH.max(settled_confirmations as usize + REORG_MARGIN)
}

/// How long a coin waits: the chain's rules, and with Manual Sync the setting too.
pub(crate) fn needed(chain_rules: u64, settings: ManualSettings) -> u64 {
    if settings.on { chain_rules.max(u64::from(settings.settled_confirmations)) } else { chain_rules }
}

/// What a pull found, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullOutcome {
    pub title: String,
    pub line: String,
}

/// The words for a pull: "1 new payment · 2 checked", then what's ready and,
/// as a pull looks only so far back, that Sync goes further.
pub(crate) fn pull_words(mode: PullMode, checked: usize, new: usize, ready: Option<String>, chain_changed: bool, not_confirmed: usize) -> PullOutcome {
    if chain_changed {
        return PullOutcome { title: "The chain changed".into(), line: "Sync to update your balance.".into() };
    }
    if not_confirmed > 0 {
        let title = if not_confirmed == 1 { "1 payment not confirmed".to_owned() } else { format!("{not_confirmed} payments not confirmed") };
        return PullOutcome { title, line: "Open it in Activity to see what to do.".into() };
    }
    let title = match (new, checked) {
        (0, 0) if ready.is_some() => "Ready to spend".to_owned(),
        (0, 0) => "Nothing new".to_owned(),
        (0, c) => format!("{} checked", payments(c)),
        (1, 0) => "1 new payment".to_owned(),
        (n, 0) => format!("{n} new payments"),
        (1, c) => format!("1 new payment · {c} checked"),
        (n, c) => format!("{n} new payments · {c} checked"),
    };
    let further = match mode {
        PullMode::Auto => "Sync to check further back.",
        _ => "Sync to look for new payments.",
    };
    let line = ready.map_or_else(|| further.to_owned(), |amount| format!("{amount} XMR is ready to spend. {further}"));
    PullOutcome { title, line }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(on: bool, settled: u32) -> ManualSettings {
        ManualSettings { on, pull_mode: PullMode::Auto, incoming_scan_blocks: 5, settled_confirmations: settled }
    }

    #[test]
    fn a_scan_replaces_what_it_covers_only_when_it_leaves_no_gap() {
        // Older blocks to the newest: scanned again.
        assert_eq!(scan_plan(50, 99, 90, 100), ScanPlan::Rewind(50));
        assert_eq!(scan_plan(90, 200, 90, 100), ScanPlan::Rewind(90), "an end past the chain's is the newest");
        // A closed range, or one past Last scan: only added to.
        assert_eq!(scan_plan(50, 60, 90, 100), ScanPlan::Window(50, 60));
        assert_eq!(scan_plan(95, 99, 90, 100), ScanPlan::Window(95, 99), "a gap from 90 to 95");
    }

    #[test]
    fn coins_wait_for_the_setting_only_with_manual_sync_on() {
        assert_eq!(needed(10, settings(false, 720)), 10);
        assert_eq!(needed(10, settings(true, 30)), 30);
        assert_eq!(needed(60, settings(true, 10)), 60, "a mined reward's lock still counts");
    }

    #[test]
    fn enough_hashes_are_kept_for_any_settled_point() {
        assert_eq!(keep_hashes(10), 100);
        assert_eq!(keep_hashes(70), 100);
        assert_eq!(keep_hashes(720), 750);
    }

    #[test]
    fn the_drawer_says_what_each_choice_costs() {
        let options = scan_options(settings(true, 10), 2, 3_811_200, 3_702_000, 3_812_640);
        assert_eq!(options[0].note, "2 payments · Pool · Last 5 blocks");
        assert_eq!(options[0].action, "Check payments");
        assert_eq!((options[1].label.as_str(), options[1].note.as_str(), options[1].cost.as_str()), ("From last scan", "1,440 blocks", "~7 s"));
        assert_eq!(options[1].action, "Scan 1,440 blocks");
        assert_eq!((options[3].note.as_str(), options[3].cost.as_str()), ("~5,040 blocks", "~25 s"));
        assert_eq!(options[5].note, "From block 3,702,000 · ~110,640 blocks");
        assert_eq!(options[5].cost, "~9 min");
        let send = scan_options(ManualSettings { pull_mode: PullMode::Send, ..settings(true, 10) }, 1, 0, 0, 0);
        assert_eq!(send[0].note, "1 payment");
        let young = scan_options(settings(true, 10), 0, 90, 90, 100);
        assert_eq!(young[2].note, "~10 blocks", "not past the wallet's first block");
    }

    #[test]
    fn a_pull_says_what_it_found_and_what_it_did_not_look_for() {
        let words = pull_words(PullMode::Auto, 2, 1, Some("0.5".into()), false, 0);
        assert_eq!(words.title, "1 new payment · 2 checked");
        assert_eq!(words.line, "0.5 XMR is ready to spend. Sync to check further back.");
        assert_eq!(pull_words(PullMode::Auto, 0, 0, None, false, 0).title, "Nothing new");
        assert_eq!(pull_words(PullMode::Auto, 0, 0, Some("0.5".into()), false, 0).title, "Ready to spend");
        assert_eq!(pull_words(PullMode::Send, 2, 0, None, false, 0).line, "Sync to look for new payments.");
        assert_eq!(pull_words(PullMode::Auto, 2, 1, None, true, 0).title, "The chain changed");
        assert_eq!(pull_words(PullMode::Auto, 2, 1, None, false, 1).title, "1 payment not confirmed");
    }

    #[test]
    fn numbers_read_in_groups() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(3_811_200), "3,811,200");
        assert_eq!(estimate(0), "~1 s");
        assert_eq!(estimate(21_600), "~2 min");
    }
}
