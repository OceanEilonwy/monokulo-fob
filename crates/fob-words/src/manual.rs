//! Manual Sync, in words.

use fob_monero::amount::grouped;
use fob_screens::{
    catch_up::Notice,
    manual::{self, BlockInfo, LastScan, PullMode, PullOutcome, ScanLabel, ScanNote, ScanOption, ScanTitle},
};

use crate::catch_up::date_time;

/// "Scanning from last scan".
pub fn scan_title(title: ScanTitle) -> String {
    match title {
        ScanTitle::FromLast => "Scanning from last scan".into(),
        ScanTitle::WholeWallet => "Scanning the whole wallet".into(),
        ScanTitle::Newest(blocks) => format!("Scanning the newest {} blocks", grouped(blocks)),
        ScanTitle::FromBlock(block) => format!("Scanning from block {}", grouped(block)),
        ScanTitle::Dates => "Scanning those dates".into(),
    }
}

/// "893 of 1,440 blocks".
pub fn scan_blocks(done: u64, total: u64) -> String {
    format!("{} of {} blocks", grouped(done), grouped(total))
}

/// Something to say once.
pub fn notice(notice: Notice) -> String {
    match notice {
        Notice::ChainFollowed => "The chain changed, so Fob scanned recent blocks again.".into(),
    }
}

/// How long `blocks` take to scan: "~7 s", "~2 min".
pub fn estimate(blocks: u64) -> String {
    let seconds = manual::seconds_to_scan(blocks);
    if seconds < 60 { format!("~{seconds} s") } else { format!("~{} min", (seconds + 30) / 60) }
}

fn payments(n: usize) -> String {
    if n == 1 { "1 payment".into() } else { format!("{n} payments") }
}

/// One of the Scan drawer's quick choices, as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Option_ {
    pub label: String,
    pub note: String,
    pub cost: String,
    pub action: String,
}

pub fn option(o: &ScanOption) -> Option_ {
    let label = match o.label {
        ScanLabel::Unsettled => "Unsettled payments",
        ScanLabel::FromLast => "From last scan",
        ScanLabel::Today => "Today",
        ScanLabel::PastWeek => "Past week",
        ScanLabel::PastMonth => "Past month",
        ScanLabel::WholeWallet => "The whole wallet",
    };
    let note = match o.note {
        ScanNote::Payments { payments: n, newest_blocks: Some(blocks) } => format!("{} · Pool · Last {blocks} blocks", payments(n)),
        ScanNote::Payments { payments: n, newest_blocks: None } => payments(n),
        ScanNote::Blocks(blocks) => format!("{} blocks", grouped(blocks)),
        ScanNote::AboutBlocks(blocks) => format!("~{} blocks", grouped(blocks)),
        ScanNote::Whole { first, blocks } => format!("From block {} · ~{} blocks", grouped(first), grouped(blocks)),
    };
    let (cost, action) = match o.blocks {
        Some(blocks) => (estimate(blocks), format!("Scan {} blocks", grouped(blocks))),
        None => ("~1 s".to_owned(), "Check payments".to_owned()),
    };
    Option_ { label: label.into(), note, cost, action }
}

/// "Last scan Today, 14:20 · Block 3,811,200", or that there hasn't been one.
pub fn last_scan(last: Option<&LastScan>) -> String {
    match last {
        Some(last) => format!("Last scan {} · Block {}", date_time(&last.when), grouped(last.block)),
        None => "No scan yet".into(),
    }
}

/// The Block tab's facts about a typed block, as they're said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// "~30 Oct 2026, 14:20", when it can be told.
    pub mined: Option<String>,
    pub newest: String,
    pub first: String,
}

pub fn block(info: &BlockInfo) -> Block {
    Block { mined: info.mined.as_ref().map(|when| format!("~{}", date_time(when))), newest: grouped(info.newest), first: grouped(info.first) }
}

/// What a pull found, as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    pub title: String,
    pub line: String,
}

/// The words for a pull: "1 new payment · 2 checked", then what's ready and,
/// as a pull looks only so far back, that Sync goes further.
pub fn pull(found: &PullOutcome) -> Pull {
    let PullOutcome { mode, checked, new, ready, chain_changed, not_confirmed } = found;
    let (checked, new, not_confirmed) = (*checked, *new, *not_confirmed);
    if *chain_changed {
        return Pull { title: "The chain changed".into(), line: "Sync to update your balance.".into() };
    }
    if not_confirmed > 0 {
        let title = if not_confirmed == 1 { "1 payment not confirmed".to_owned() } else { format!("{not_confirmed} payments not confirmed") };
        return Pull { title, line: "Open it in Activity to see what to do.".into() };
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
    let line = ready.as_ref().map_or_else(|| further.to_owned(), |amount| format!("{amount} XMR is ready to spend. {further}"));
    Pull { title, line }
}

#[cfg(test)]
mod tests {
    use fob_screens::manual::{ManualSettings, scan_options};

    use super::*;

    fn settings() -> ManualSettings {
        ManualSettings { on: true, pull_mode: PullMode::Auto, incoming_scan_blocks: 5, settled_confirmations: 10 }
    }

    fn found(mode: PullMode, checked: usize, new: usize, ready: Option<&str>, chain_changed: bool, not_confirmed: usize) -> Pull {
        pull(&PullOutcome { mode, checked, new, ready: ready.map(str::to_owned), chain_changed, not_confirmed })
    }

    #[test]
    fn the_drawer_says_what_each_choice_costs() {
        let options: Vec<Option_> = scan_options(settings(), 2, 3_811_200, 3_702_000, 3_812_640).iter().map(option).collect();
        assert_eq!(options[0].note, "2 payments · Pool · Last 5 blocks");
        assert_eq!(options[0].action, "Check payments");
        assert_eq!((options[1].label.as_str(), options[1].note.as_str(), options[1].cost.as_str()), ("From last scan", "1,440 blocks", "~7 s"));
        assert_eq!(options[1].action, "Scan 1,440 blocks");
        assert_eq!((options[3].note.as_str(), options[3].cost.as_str()), ("~5,040 blocks", "~25 s"));
        assert_eq!(options[5].note, "From block 3,702,000 · ~110,640 blocks");
        assert_eq!(options[5].cost, "~9 min");
        let send = scan_options(ManualSettings { pull_mode: PullMode::Send, ..settings() }, 1, 0, 0, 0);
        assert_eq!(option(&send[0]).note, "1 payment");
        let young = scan_options(settings(), 0, 90, 90, 100);
        assert_eq!(option(&young[2]).note, "~10 blocks", "not past the wallet's first block");
    }

    #[test]
    fn a_pull_says_what_it_found_and_what_it_did_not_look_for() {
        let words = found(PullMode::Auto, 2, 1, Some("0.5"), false, 0);
        assert_eq!(words.title, "1 new payment · 2 checked");
        assert_eq!(words.line, "0.5 XMR is ready to spend. Sync to check further back.");
        assert_eq!(found(PullMode::Auto, 0, 0, None, false, 0).title, "Nothing new");
        assert_eq!(found(PullMode::Auto, 0, 0, Some("0.5"), false, 0).title, "Ready to spend");
        assert_eq!(found(PullMode::Send, 2, 0, None, false, 0).line, "Sync to look for new payments.");
        assert_eq!(found(PullMode::Auto, 2, 1, None, true, 0).title, "The chain changed");
        assert_eq!(found(PullMode::Auto, 2, 1, None, false, 1).title, "1 payment not confirmed");
    }

    #[test]
    fn numbers_read_in_groups() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(3_811_200), "3,811,200");
        assert_eq!(estimate(0), "~1 s");
        assert_eq!(estimate(21_600), "~2 min");
    }

    #[test]
    fn a_scan_says_what_it_is_doing() {
        assert_eq!(scan_title(ScanTitle::FromLast), "Scanning from last scan");
        assert_eq!(scan_title(ScanTitle::WholeWallet), "Scanning the whole wallet");
        assert_eq!(scan_title(ScanTitle::Newest(5_040)), "Scanning the newest 5,040 blocks");
        assert_eq!(scan_title(ScanTitle::FromBlock(3_811_200)), "Scanning from block 3,811,200");
        assert_eq!(scan_title(ScanTitle::Dates), "Scanning those dates");
        assert_eq!(scan_blocks(893, 1_440), "893 of 1,440 blocks");
        assert_eq!(notice(Notice::ChainFollowed), "The chain changed, so Fob scanned recent blocks again.");
        assert_eq!(last_scan(None), "No scan yet");
    }
}
