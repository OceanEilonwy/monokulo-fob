//! How catching up looks: the chip in Home's top bar and the details card
//! under it. Speeds come from a short window of what the sync loop saw
//! (blocks scanned, bytes the fetcher took from the node), and the time
//! left from a longer one, counting down with the clock, never up, and
//! rounded so it reads calmly.

use std::{collections::VecDeque, time::Duration, time::Instant};

use fob_monero::network::ZmqStatus;
use crate::{clock::{self, Clock}, manual::ScanTitle};

/// Speeds are measured over about this much of the latest samples.
const SPAN: Duration = Duration::from_secs(5);
/// The time left is estimated over about this much: block sizes vary a
/// lot along the chain, and a 5 s window's estimate wobbles with them.
const SMOOTH: Duration = Duration::from_secs(20);
/// No speed, so no time left, until this much has been measured.
const SETTLE: Duration = Duration::from_secs(3);
/// A first time left under this isn't shown: a catch-up that's nearly over
/// shows only Starting, then Synced.
const FIRST_SHOWN: u64 = 10;
/// The time left shown is dropped when the estimate rises past it by half
/// as much again and this many seconds more (a node that slows a lot)...
const RISE: u64 = 30;
/// ...and a new one counts down from the estimate once this much has been
/// measured since.
const RESETTLE: Duration = Duration::from_secs(10);

/// Blocks this slow to come are worth saying so.
pub const WAIT_SHOWN: u64 = 15;

/// One look at the sync loop: when, and its running totals.
#[derive(Clone, Copy, Debug)]
struct Sample {
    at: Instant,
    bytes: u64,
    blocks: u64,
}

/// What one catch-up has measured: blocks per second, the link's bytes per
/// second, and the time left as shown.
#[derive(Debug, Default)]
pub struct Pace {
    started: Option<Instant>,
    samples: VecDeque<Sample>,
    /// The transport's count and the scanned height at the last sample, to
    /// add only what came since (a new transport counts from 0 again).
    last: Option<(u64, u64)>,
    /// The time left as set, and when: it counts down with the clock from there.
    shown: Option<(u64, Instant)>,
    /// When the time left was dropped for a big rise (or a stall).
    dropped: Option<Instant>,
}

/// Speeds once settled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speeds {
    pub blocks_per_second: f64,
    pub bytes_per_second: f64,
}

impl Pace {
    /// A new catch-up, or one after a failure or a wait: nothing measured.
    pub fn reset(&mut self) {
        *self = Pace::default();
    }

    /// Notes the transport's `received` bytes and the `scanned` height at
    /// `at`, with `left` blocks to go, and returns the time left to show.
    pub fn sample(&mut self, at: Instant, received: u64, scanned: u64, left: u64) -> Option<u64> {
        let (bytes, blocks) = match self.last {
            // The count starts again with a new transport; a rewind scans again.
            Some((r, s)) => (if received >= r { received - r } else { received }, scanned.saturating_sub(s)),
            None => (0, 0),
        };
        self.last = Some((received, scanned));
        let total = self.samples.back().map_or(Sample { at, bytes: 0, blocks: 0 }, |s| Sample { at, bytes: s.bytes + bytes, blocks: s.blocks + blocks });
        self.started.get_or_insert(at);
        self.samples.push_back(total);
        // Keep one sample from before the longer window, as its base.
        while self.samples.len() > 2 && self.samples[1].at + SMOOTH <= at {
            self.samples.pop_front();
        }
        self.next_shown(at, left);
        self.held(at)
    }

    /// Speeds over the window, once [`SETTLE`] has been measured.
    pub fn speeds(&self) -> Option<Speeds> {
        self.over(SPAN)
    }

    /// Speeds from the newest sample back to the last one at least `span`
    /// older (or the oldest kept), once [`SETTLE`] has been measured.
    fn over(&self, span: Duration) -> Option<Speeds> {
        let last = self.samples.back()?;
        if last.at.duration_since(self.started?) < SETTLE {
            return None;
        }
        let first = self.samples.iter().rev().find(|s| s.at + span <= last.at).unwrap_or(self.samples.front()?);
        let seconds = last.at.duration_since(first.at).as_secs_f64();
        (seconds > 0.0).then(|| Speeds {
            blocks_per_second: (last.blocks - first.blocks) as f64 / seconds,
            bytes_per_second: (last.bytes - first.bytes) as f64 / seconds,
        })
    }

    /// The time left shown at `at`: what was set, less the time since, and
    /// never under a second while there's something left.
    fn held(&self, at: Instant) -> Option<u64> {
        self.shown.map(|(seconds, set)| seconds.saturating_sub(at.saturating_duration_since(set).as_secs()).max(1))
    }

    /// Moves the time left on for `left` blocks at `at`: none until the
    /// speed settles; then it counts down with the clock, taking a lower
    /// estimate at once and holding through a higher one. It is never
    /// raised: an estimate half as much again and [`RISE`] more drops it
    /// ("Working it out"), and a fresh one counts down once [`RESETTLE`]
    /// more has been measured. A stall over the whole window drops it too.
    fn next_shown(&mut self, at: Instant, left: u64) {
        if left == 0 {
            self.shown = None;
            return;
        }
        let Some(speeds) = self.over(SMOOTH) else {
            self.shown = None;
            return;
        };
        let estimate = (speeds.blocks_per_second > 0.0).then(|| (left as f64 / speeds.blocks_per_second).ceil() as u64);
        match (self.held(at), estimate) {
            (_, None) => {
                self.shown = None;
                self.dropped = Some(at);
            }
            (Some(held), Some(estimate)) if estimate <= held => self.shown = Some((estimate, at)),
            (Some(held), Some(estimate)) if estimate >= held + held / 2 && estimate >= held + RISE => {
                self.shown = None;
                self.dropped = Some(at);
            }
            (Some(_), Some(_)) => {}
            (None, Some(_)) if self.dropped.is_some_and(|d| at.duration_since(d) < RESETTLE) => {}
            (None, Some(estimate)) if estimate < FIRST_SHOWN => {}
            (None, Some(estimate)) => {
                self.shown = Some((estimate, at));
                self.dropped = None;
            }
        }
    }

    /// Notes a report: on a failure nothing is sampled and the measuring
    /// starts again, so the first sample after the wait is the base and no
    /// speed spans the time spent waiting. Gives the time left and speeds.
    pub fn report(&mut self, at: Instant, received: u64, scanned: u64, left: u64, failed: bool) -> (Option<u64>, Option<Speeds>) {
        if failed {
            self.reset();
            return (None, None);
        }
        let seconds_left = self.sample(at, received, scanned, left);
        (seconds_left, self.speeds())
    }
}

/// Seconds rounded for reading: up to the next 10 s under a minute, the
/// nearest minute under an hour, the nearest 5 minutes under 10 hours, then
/// whole hours. Never less than 10 s.
pub fn rounded(seconds: u64) -> u64 {
    let seconds = seconds.max(1);
    if seconds < 60 {
        seconds.div_ceil(10) * 10
    } else if seconds < 3600 {
        (seconds + 30) / 60 * 60
    } else if seconds < 36_000 {
        (seconds + 150) / 300 * 300
    } else {
        (seconds + 1800) / 3600 * 3600
    }
}

/// How the chip looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A ring filled as far as the progress.
    Ring,
    /// A green dot.
    Synced,
    /// A red dot.
    Offline,
}

/// The chip in Home's top bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub look: Look,
    pub state: SyncState,
}

/// Where catching up stands, for the chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncState {
    /// The last round failed: the phone's own fault (`local`), or the node's.
    Failed { local: bool },
    /// The chain changed further back than the wallet can check.
    ChainChanged,
    Connecting,
    Synced,
    /// The node is keeping the wallet waiting; how far it got, in percent.
    Waiting { percent: u32 },
    /// The time left, with how far it got.
    Left { seconds: u64, percent: u32 },
    /// Measured, but the time left dropped for a slowdown: no number.
    CatchingUp { percent: u32 },
    Starting { percent: u32 },
}

/// Percent done, never 100 before the end.
fn percent(status: &SyncStatus) -> u32 {
    let p = (status.progress.clamp(0.0, 1.0) * 100.0).floor() as u32;
    if status.synced { p } else { p.min(99) }
}

impl SyncStatus {
    pub fn chip(&self) -> Chip {
        if self.error.is_some() {
            if self.chain_changed {
                return Chip { look: Look::Offline, state: SyncState::ChainChanged };
            }
            return Chip { look: Look::Offline, state: SyncState::Failed { local: self.error_local } };
        }
        // First: Synced from before the node has answered would be a guess.
        if !self.connected {
            return Chip { look: Look::Ring, state: SyncState::Connecting };
        }
        if self.synced {
            return Chip { look: Look::Synced, state: SyncState::Synced };
        }
        let percent = percent(self);
        let state = match self.seconds_left {
            _ if self.waiting_seconds >= WAIT_SHOWN => SyncState::Waiting { percent },
            Some(seconds) => SyncState::Left { seconds, percent },
            None if self.blocks_per_second.is_some() => SyncState::CatchingUp { percent },
            None => SyncState::Starting { percent },
        };
        Chip { look: Look::Ring, state }
    }
}

/// One of the card's rows.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// How long; `None` until the speed settles.
    TimeLeft(Option<u64>),
    /// When the last block scanned was mined.
    PaymentsUpTo(clock::When),
    /// Blocks done of those to do, and how fast once it's known.
    Blocks { done: u64, of: u64, per_second: Option<f64> },
    /// Bytes a second; `None` until it's measured.
    LinkSpeed(Option<f64>),
    LatestBlock(u64),
}

/// The card's title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Title {
    Failed { local: bool },
    ChainChanged,
    Connecting,
    Synced,
    Waiting,
    CatchingUp,
}

/// What the card adds under its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    KeepsTrying,
    /// The node may be on the wrong chain, or the chain really did change.
    WrongChain,
    AskingHowFar,
    /// The node hasn't sent any blocks for this long.
    NoBlocksFor { seconds: u64 },
}

/// The card the chip opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    pub title: Title,
    /// Percent beside the title while catching up.
    pub percent: Option<u32>,
    /// The bar, while catching up.
    pub progress: Option<f32>,
    pub rows: Vec<Row>,
    pub note: Option<Note>,
    /// What went wrong, as the node or the network said it.
    pub error: Option<String>,
    /// Scanning the whole wallet again is the way past the failure.
    pub scan_again: bool,
}

/// The card for `status`.
pub fn details(status: &SyncStatus, clock: Clock) -> Details {
    let speeds = status.blocks_per_second.zip(status.link_mb_per_second);
    let waiting = status.connected && !status.synced && status.waiting_seconds >= WAIT_SHOWN;
    let catching_up = !status.synced && status.tip > 0 && status.connected;
    let mut rows = vec![];
    if catching_up && status.error.is_none() && !waiting {
        rows.push(Row::TimeLeft(status.seconds_left));
    }
    if status.up_to > 0 {
        rows.push(Row::PaymentsUpTo(clock::when(status.up_to, clock)));
    }
    if catching_up {
        rows.push(Row::Blocks {
            done: status.scanned.saturating_sub(status.start),
            of: status.tip.saturating_sub(status.start),
            per_second: speeds.filter(|_| status.error.is_none()).map(|(per, _)| per),
        });
        if status.error.is_none() && !waiting {
            rows.push(Row::LinkSpeed(speeds.map(|(_, mb)| mb * 1e6)));
        }
    } else if status.tip > 0 && status.connected {
        rows.push(Row::LatestBlock(status.tip - 1));
    }
    let title = match () {
        _ if status.chain_changed && status.error.is_some() => Title::ChainChanged,
        _ if status.error.is_some() => Title::Failed { local: status.error_local },
        _ if !status.connected => Title::Connecting,
        _ if status.synced => Title::Synced,
        _ if waiting => Title::Waiting,
        _ => Title::CatchingUp,
    };
    let note = match () {
        _ if status.chain_changed && status.error.is_some() => Some(Note::WrongChain),
        _ if status.error.is_some() => Some(Note::KeepsTrying),
        _ if !status.connected => Some(Note::AskingHowFar),
        _ if waiting => Some(Note::NoBlocksFor { seconds: status.waiting_seconds }),
        _ => None,
    };
    let unsure = !status.connected && status.error.is_none();
    Details {
        title,
        percent: (!status.synced && !unsure).then(|| percent(status)),
        progress: (!status.synced && !unsure).then_some(status.progress.clamp(0.0, 1.0)),
        rows,
        note,
        error: status.error.clone(),
        scan_again: status.chain_changed && status.error.is_some(),
    }
}

/// Something to say once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A reorganisation was followed: recent blocks were scanned again.
    ChainFollowed,
}

/// How far a scan someone asked for has got.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScanProgress {
    pub title: ScanTitle,
    pub from: u64,
    pub to: u64,
    /// The next block it will scan.
    pub at: u64,
    /// Blocks with payments found so far, and whether money came in. A block
    /// with both is marked as money in; change isn't money in.
    pub marks: Vec<(u64, bool)>,
}

/// How far syncing has got.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncStatus {
    pub scanned: u64,
    pub tip: u64,
    pub start: u64,
    /// As far as the wallet knows: `tip` may be from the last time it asked,
    /// until [`connected`](Self::connected) says the node has answered.
    pub synced: bool,
    /// The node has answered since syncing started, and hasn't failed since.
    pub connected: bool,
    /// How long this wallet has been waiting for the node's next blocks; 0
    /// when they're coming.
    pub waiting_seconds: u64,
    /// 0 to 1, from the restore height to the tip.
    pub progress: f32,
    /// The time left as the chip shows it: none until the speed settles,
    /// counting down, and never up (see [`catch_up`]).
    pub seconds_left: Option<u64>,
    /// Blocks scanned per second over the last few seconds, once settled.
    pub blocks_per_second: Option<f64>,
    /// Megabytes a second the fetcher took from the node over the last few
    /// seconds, once settled.
    pub link_mb_per_second: Option<f64>,
    /// When the last scanned block was mined.
    pub up_to: u64,
    /// Why the last round failed, as `fob-words` said the error, or as the
    /// node said it.
    pub error: Option<String>,
    /// The failure is the phone's own (the wallet couldn't be saved), not
    /// the node's.
    pub error_local: bool,
    /// The failure is a reorganisation deeper than the wallet can check.
    pub chain_changed: bool,
    /// Manual Sync is on.
    pub manual_sync: bool,
    /// A scan someone asked for, while it runs.
    pub scan: Option<ScanProgress>,
    /// Something to say once.
    pub notice: Option<Notice>,
    /// Instant updates from the node's ZMQ publisher.
    pub zmq: ZmqStatus,
    /// Where instant updates come from, when set.
    pub zmq_endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    /// Samples every half second at `bps` blocks and `bytes` per second
    /// from `t0 + from` to `t0 + to`, `tip` the end; returns the last shown.
    fn run(pace: &mut Pace, t0: Instant, (from, to): (f64, f64), (bps, bytes): (f64, f64), start: (u64, u64), tip: u64) -> (Option<u64>, (u64, u64)) {
        let (mut received, mut scanned) = start;
        let mut shown = None;
        let mut t = from;
        while t <= to + 1e-9 {
            shown = pace.sample(t0 + secs(t), received, scanned, tip.saturating_sub(scanned));
            received += (bytes * 0.5) as u64;
            scanned += (bps * 0.5) as u64;
            t += 0.5;
        }
        (shown, (received, scanned))
    }




    /// Every time left shown, sampling every half second at the speeds
    /// `bps(t)` blocks a second, from `t0 + from` to `t0 + to`.
    fn trace(pace: &mut Pace, t0: Instant, (from, to): (f64, f64), bps: impl Fn(f64) -> f64, start: u64, tip: u64) -> (Vec<Option<u64>>, u64) {
        let (mut scanned, mut done, mut shown) = (start, start as f64, vec![]);
        let mut t = from;
        while t <= to + 1e-9 {
            shown.push(pace.sample(t0 + secs(t), scanned * 50_000, scanned, tip.saturating_sub(scanned)));
            done += bps(t) * 0.5;
            scanned = done as u64;
            t += 0.5;
        }
        (shown, scanned)
    }

    /// No number shown is ever larger than the one shown just before it.
    fn never_up(shown: &[Option<u64>]) {
        for pair in shown.windows(2) {
            if let [Some(a), Some(b)] = pair {
                assert!(b <= a, "{a} then {b} in {shown:?}");
            }
        }
    }


    /// Block sizes vary along the chain: the speed over a few seconds wobbles
    /// by a fifth either way. The estimate, over a longer window, doesn't
    /// rise past the shown time enough to drop it, and the time counts down.

    /// A node that slows a lot: the number isn't raised, it's dropped
    /// ("Working it out"), and a fresh countdown starts once settled.

    /// After a failure the measuring starts again from the first report
    /// after the wait: no speed spans the wait, so the first time left
    /// isn't hours for a minute's backoff.






    /// Back on screen with the wallet caught up as of last time, or just on
    /// another node: it isn't Synced until the node has said so.

    /// A node that answers but sends nothing says so, in place of a time left
    /// counting down on nothing.

    #[test]
    fn no_speed_until_three_seconds_are_measured() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (shown, _) = run(&mut pace, t0, (0.0, 2.5), (20.0, 1e6), (0, 0), 100_000);
        assert_eq!((shown, pace.speeds()), (None, None));
        pace.sample(t0 + secs(3.0), 3_000_000, 60, 100_000 - 60);
        let s = pace.speeds().unwrap();
        assert!((s.blocks_per_second - 20.0).abs() < 1e-9 && (s.bytes_per_second - 1e6).abs() < 1e-3, "{s:?}");
    }

    #[test]
    fn speeds_cover_only_the_last_five_seconds() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (_, at) = run(&mut pace, t0, (0.0, 10.0), (100.0, 4e6), (0, 0), 1_000_000);
        // Then it slows to a tenth: after 5 s only the slow pace counts.
        run(&mut pace, t0, (10.5, 16.0), (10.0, 4e5), at, 1_000_000);
        let s = pace.speeds().unwrap();
        assert!((s.blocks_per_second - 10.0).abs() < 1.0, "{s:?}");
        assert!((s.bytes_per_second - 4e5).abs() < 4e4, "{s:?}");
    }

    #[test]
    fn a_new_transport_counting_from_zero_adds_its_own_bytes() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        pace.sample(t0, 5_000_000, 100, 1000);
        pace.sample(t0 + secs(2.0), 7_000_000, 140, 960);
        pace.sample(t0 + secs(4.0), 2_000_000, 180, 920);
        let s = pace.speeds().unwrap();
        assert!((s.bytes_per_second - 1e6).abs() < 1e-3, "{s:?}");
        assert!((s.blocks_per_second - 20.0).abs() < 1e-9);
    }

    #[test]
    fn the_time_left_counts_down_and_never_up_for_a_wobble() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        // 10,000 blocks at 20/s: about 500 s.
        let (shown, at) = trace(&mut pace, t0, (0.0, 4.0), |_| 20.0, 0, 10_000);
        let first = shown.last().unwrap().unwrap();
        assert!((480..=500).contains(&first), "{first}");
        // A bit slower (16/s, an estimate about a quarter higher): held,
        // and the held time goes on counting down with the clock.
        let (slower, at) = trace(&mut pace, t0, (4.5, 12.0), |_| 16.0, at, 10_000);
        never_up(&[shown.clone(), slower.clone()].concat());
        let held = slower.last().unwrap().unwrap();
        assert!((first - 9..=first - 7).contains(&held), "{held} 8 s after {first}");
        // Back to 20/s: it counts down from there.
        let (back, _) = trace(&mut pace, t0, (12.5, 40.0), |_| 20.0, at, 10_000);
        never_up(&[slower, back.clone()].concat());
        assert!(back.iter().all(Option::is_some), "never dropped for a wobble");
        assert!(back.last().unwrap().unwrap() < held);
    }

    #[test]
    fn ordinary_noise_neither_drops_nor_freezes_the_time_left() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let noisy = |t: f64| if (t * 2.0) as u64 % 6 < 3 { 24.0 } else { 16.0 };
        let (shown, _) = trace(&mut pace, t0, (0.0, 120.0), noisy, 0, 10_000);
        never_up(&shown);
        let shown: Vec<u64> = shown.into_iter().skip_while(Option::is_none).map(|s| s.expect("never dropped")).collect();
        // About 500 s at first, about 380 s two minutes on.
        assert!((440..=560).contains(&shown[0]), "{}", shown[0]);
        assert!((340..=420).contains(shown.last().unwrap()), "{}", shown.last().unwrap());
    }

    #[test]
    fn a_big_rise_drops_the_time_left_and_counts_afresh() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (fast, at) = trace(&mut pace, t0, (0.0, 20.0), |_| 40.0, 0, 10_000);
        let first = fast.last().unwrap().unwrap();
        // A quarter of the speed: about four times as long.
        let (slow, _) = trace(&mut pace, t0, (20.5, 80.0), |_| 10.0, at, 10_000);
        never_up(&[fast, slow.clone()].concat());
        let dropped = slow.iter().position(Option::is_none).expect("dropped");
        let fresh = slow[dropped..].iter().position(Option::is_some).expect("counts again") + dropped;
        assert!(fresh - dropped >= 20, "settles {} s before counting again", (fresh - dropped) / 2);
        assert!(slow[fresh].unwrap() > first * 2, "{:?} after {first}", slow[fresh]);
        never_up(&slow[fresh..]);
        assert!(slow[fresh..].iter().all(Option::is_some));
    }

    #[test]
    fn a_failure_starts_the_measuring_again_after_the_wait() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (_, (received, scanned)) = run(&mut pace, t0, (0.0, 10.0), (50.0, 1e6), (0, 0), 20_000);
        assert_eq!(pace.report(t0 + secs(10.5), received, scanned, 20_000 - scanned, true), (None, None));
        // A minute's backoff, then 100 blocks a step again.
        let mut at = (received, scanned);
        for i in 0..6 {
            let t = 70.5 + i as f64 * 0.5;
            assert_eq!(pace.report(t0 + secs(t), at.0, at.1, 20_000 - at.1, false), (None, None), "{t} s: still settling");
            at = (at.0 + 500_000, at.1 + 25);
        }
        let (left, speeds) = pace.report(t0 + secs(73.5), at.0, at.1, 20_000 - at.1, false);
        let speeds = speeds.expect("settled 3 s after recovery");
        assert!((speeds.blocks_per_second - 50.0).abs() < 1e-9, "{speeds:?}");
        assert!(left.unwrap() < 400, "{left:?}");
    }

    #[test]
    fn a_short_catch_up_shows_no_time() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        // 200 blocks at 40/s: done in 5 s, never a time.
        let mut scanned = 0;
        for i in 0..=10 {
            assert_eq!(pace.sample(t0 + secs(i as f64 * 0.5), scanned * 1000, scanned, 200 - scanned), None, "at {i}");
            scanned = (scanned + 20).min(200);
        }
        // Even one that's barely started: the first time shown is at least 10 s.
        let mut pace = Pace::default();
        let (shown, _) = run(&mut pace, t0, (0.0, 3.0), (100.0, 1e6), (0, 0), 1_000);
        assert_eq!(shown, None);
    }

    #[test]
    fn a_stall_drops_the_time_left_rather_than_count_to_nothing() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (shown, at) = trace(&mut pace, t0, (0.0, 10.0), |_| 20.0, 0, 10_000);
        assert!(shown.last().unwrap().is_some());
        // Nothing at all from then on: no number, and no guess.
        let (stalled, _) = trace(&mut pace, t0, (10.5, 40.0), |_| 0.0, at, 10_000);
        never_up(&[shown, stalled.clone()].concat());
        assert_eq!(*stalled.last().unwrap(), None);
    }

    #[test]
    fn reset_forgets_the_last_catch_up() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        run(&mut pace, t0, (0.0, 4.0), (20.0, 1e6), (0, 0), 10_000);
        pace.reset();
        assert_eq!(pace.sample(t0 + secs(60.0), 0, 200, 9_800), None);
        assert_eq!(pace.speeds(), None);
    }

    #[test]
    fn the_chip_and_card_say_where_it_stands_without_words() {
        let status = SyncStatus { connected: true, tip: 100, start: 0, scanned: 34, progress: 0.34, seconds_left: Some(370), ..SyncStatus::default() };
        assert_eq!(status.chip().state, SyncState::Left { seconds: 370, percent: 34 });
        let card = details(&status, Clock { now: 1_793_448_000, utc_offset: 0, hours_24: true });
        assert_eq!((card.title, card.percent), (Title::CatchingUp, Some(34)));
        assert_eq!(card.rows[0], Row::TimeLeft(Some(370)));
        let failed = SyncStatus { error: Some("no".into()), error_local: true, ..SyncStatus::default() };
        assert_eq!(failed.chip().state, SyncState::Failed { local: true });
        let changed = SyncStatus { error: Some("no".into()), chain_changed: true, ..SyncStatus::default() };
        assert_eq!((changed.chip().look, changed.chip().state), (Look::Offline, SyncState::ChainChanged));
        let card = details(&changed, Clock { now: 1_793_448_000, utc_offset: 0, hours_24: true });
        assert_eq!((card.title, card.note, card.scan_again), (Title::ChainChanged, Some(Note::WrongChain), true));
    }
}
