//! Manual Sync: a wallet that only scans when asked, and what asking can mean.
//!
//! With it on, nothing looks for blocks by itself. Sync opens the Scan drawer,
//! whose choices are here. Pulling down on Home does what [`PullMode`] says.
//! Coins become spendable at [`ManualSettings::settled_confirmations`].

use serde::{Deserialize, Serialize};

use crate::clock::When;

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

pub const BLOCKS_A_DAY: u64 = 720;
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
pub enum ScanPlan {
    /// Forget from this block on and scan to the newest: Last scan moves.
    Rewind(u64),
    /// Scan just these, keeping everything else: Last scan stays.
    Window(u64, u64),
}

/// Blocks that end at the newest and reach Last scan can replace what's known.
/// Anything else leaves a gap, so it's only added to.
pub fn scan_plan(from: u64, to: u64, next_block: u64, tip: u64) -> ScanPlan {
    let newest = tip.saturating_sub(1);
    if to >= newest && from <= next_block { ScanPlan::Rewind(from) } else { ScanPlan::Window(from, to.min(newest)) }
}

/// What a scan someone asked for is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanTitle {
    #[default]
    FromLast,
    WholeWallet,
    /// The newest this many blocks.
    Newest(u64),
    FromBlock(u64),
    Dates,
}

/// Which of the Scan drawer's quick choices this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanLabel {
    Unsettled,
    FromLast,
    Today,
    PastWeek,
    PastMonth,
    WholeWallet,
}

/// What a quick choice covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanNote {
    /// Payments to check, and with Auto the pool and this many newest blocks.
    Payments { payments: usize, newest_blocks: Option<u32> },
    Blocks(u64),
    /// As many as it can be told.
    AboutBlocks(u64),
    Whole { first: u64, blocks: u64 },
}

/// One of the Scan drawer's quick choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanOption {
    pub request: ScanRequest,
    pub label: ScanLabel,
    pub note: ScanNote,
    /// The blocks it scans; none when it only checks payments.
    pub blocks: Option<u64>,
}

/// The Block tab's facts about a typed block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    /// About when it was mined, when it can be told.
    pub mined: Option<When>,
    pub newest: u64,
    pub first: u64,
}

/// The last scan: when the block was mined, and which it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastScan {
    pub when: When,
    pub block: u64,
}

/// About how long `blocks` take to scan, in seconds.
pub fn seconds_to_scan(blocks: u64) -> u64 {
    (blocks / BLOCKS_A_SECOND).max(1)
}

/// The drawer's quick choices.
pub fn scan_options(settings: ManualSettings, unsettled: usize, next_block: u64, first_block: u64, tip: u64) -> Vec<ScanOption> {
    let newest_blocks = (settings.pull_mode == PullMode::Auto).then_some(settings.incoming_scan_blocks);
    let behind = tip.saturating_sub(next_block);
    let whole = tip.saturating_sub(first_block);
    // Nothing before the wallet's first block is looked at.
    let back = |label, blocks: u64| {
        let about = blocks.min(whole);
        ScanOption { request: ScanRequest::Back(blocks), label, note: ScanNote::AboutBlocks(about), blocks: Some(about) }
    };
    vec![
        ScanOption { request: ScanRequest::Unsettled, label: ScanLabel::Unsettled, note: ScanNote::Payments { payments: unsettled, newest_blocks }, blocks: None },
        ScanOption { request: ScanRequest::FromLast, label: ScanLabel::FromLast, note: ScanNote::Blocks(behind), blocks: Some(behind) },
        back(ScanLabel::Today, BLOCKS_A_DAY),
        back(ScanLabel::PastWeek, BLOCKS_A_DAY * 7),
        back(ScanLabel::PastMonth, BLOCKS_A_DAY * 30),
        ScanOption { request: ScanRequest::Whole, label: ScanLabel::WholeWallet, note: ScanNote::Whole { first: first_block, blocks: whole }, blocks: Some(whole) },
    ]
}

/// How many block hashes to keep for finding reorganisations.
pub fn keep_hashes(settled_confirmations: u32) -> usize {
    REORG_DEPTH.max(settled_confirmations as usize + REORG_MARGIN)
}

/// How long a coin waits: the chain's rules, and with Manual Sync the setting too.
pub fn needed(chain_rules: u64, settings: ManualSettings) -> u64 {
    if settings.on { chain_rules.max(u64::from(settings.settled_confirmations)) } else { chain_rules }
}

/// What a pull found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullOutcome {
    pub mode: PullMode,
    /// Sent payments it checked.
    pub checked: usize,
    /// Payments it found that weren't known.
    pub new: usize,
    /// What became ready to spend, if anything, with its digits.
    pub ready: Option<String>,
    pub chain_changed: bool,
    pub not_confirmed: usize,
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
    fn the_drawer_offers_each_choice_with_its_blocks() {
        let options = scan_options(settings(true, 10), 2, 3_811_200, 3_702_000, 3_812_640);
        assert_eq!(options[0], ScanOption { request: ScanRequest::Unsettled, label: ScanLabel::Unsettled, note: ScanNote::Payments { payments: 2, newest_blocks: Some(5) }, blocks: None });
        assert_eq!((options[1].note, options[1].blocks), (ScanNote::Blocks(1_440), Some(1_440)));
        assert_eq!((options[3].request, options[3].note), (ScanRequest::Back(5_040), ScanNote::AboutBlocks(5_040)));
        assert_eq!(options[5].note, ScanNote::Whole { first: 3_702_000, blocks: 110_640 });
        let send = scan_options(ManualSettings { pull_mode: PullMode::Send, ..settings(true, 10) }, 1, 0, 0, 0);
        assert_eq!(send[0].note, ScanNote::Payments { payments: 1, newest_blocks: None });
        let young = scan_options(settings(true, 10), 0, 90, 90, 100);
        assert_eq!(young[2].note, ScanNote::AboutBlocks(10), "not past the wallet's first block");
    }
}
