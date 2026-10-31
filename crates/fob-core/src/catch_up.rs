//! How catching up looks: the chip in Home's top bar and the details card
//! under it. Speeds come from a short window of what the sync loop saw
//! (blocks scanned, bytes the fetcher took from the node), and the time
//! left from a longer one, counting down with the clock, never up, and
//! rounded so it reads calmly.

use std::{collections::VecDeque, time::Duration, time::Instant};

use crate::{
    activity::{self, Clock},
    price::grouped,
    wallet::SyncStatus,
};

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
fn rounded(seconds: u64) -> u64 {
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

/// "20 s", "6 min", "1 h 10 min", "2 h".
pub fn short_time(seconds: u64) -> String {
    let s = rounded(seconds);
    let (h, m) = (s / 3600, s % 3600 / 60);
    match () {
        _ if s < 60 => format!("{s} s"),
        _ if h == 0 => format!("{m} min"),
        _ if m == 0 => format!("{h} h"),
        _ => format!("{h} h {m} min"),
    }
}

/// The same for TalkBack: "20 seconds", "1 minute", "1 hour 10 minutes".
pub fn long_time(seconds: u64) -> String {
    let s = rounded(seconds);
    let unit = |n: u64, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    let (h, m) = (s / 3600, s % 3600 / 60);
    match () {
        _ if s < 60 => unit(s, "second"),
        _ if h == 0 => unit(m, "minute"),
        _ if m == 0 => unit(h, "hour"),
        _ => format!("{} {}", unit(h, "hour"), unit(m, "minute")),
    }
}

/// "28", or "0.8" for slow going.
fn blocks_a_second(blocks: f64) -> String {
    if blocks >= 9.95 { grouped(blocks.round() as u64) } else { format!("{blocks:.1}") }
}

/// "28/s", or "0.8/s" for slow going.
fn per_second(blocks: f64) -> String {
    format!("{}/s", blocks_a_second(blocks))
}

/// The amount and its unit: ("1.8", "MB"), ("12", "MB"), or ("640", "kB")
/// under a megabyte (decimal units).
fn link_amount(bytes_per_second: f64) -> (String, &'static str) {
    let mb = bytes_per_second / 1e6;
    match () {
        _ if mb >= 9.95 => (format!("{}", mb.round()), "MB"),
        _ if mb >= 0.995 => (format!("{mb:.1}"), "MB"),
        _ => (format!("{}", (bytes_per_second / 1e3).round()), "kB"),
    }
}

/// "1.8 MB/s", "12 MB/s", or "640 kB/s" under a megabyte (decimal units).
pub fn link_speed(bytes_per_second: f64) -> String {
    let (amount, unit) = link_amount(bytes_per_second);
    format!("{amount} {unit}/s")
}

/// The same for TalkBack: "1.8 megabytes a second".
fn link_speed_spoken(bytes_per_second: f64) -> String {
    let (amount, unit) = link_amount(bytes_per_second);
    format!("{amount} {} a second", if unit == "MB" { "megabytes" } else { "kilobytes" })
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
    /// "Starting", "6 min left", "Synced", "Can't reach node", "Can't save".
    pub text: String,
    /// What TalkBack reads: "Catching up, 34%, about 6 minutes left".
    pub label: String,
}

/// The card's title (and the chip's label) for a failure: the node's, or
/// the phone's own.
fn failed_title(status: &SyncStatus) -> &'static str {
    if status.error_local { "Can't save the wallet" } else { "Can't reach node" }
}

/// Percent done, never 100 before the end: "34%".
fn percent(status: &SyncStatus) -> String {
    let p = (status.progress.clamp(0.0, 1.0) * 100.0).floor() as u32;
    format!("{}%", if status.synced { p } else { p.min(99) })
}

impl SyncStatus {
    pub fn chip(&self) -> Chip {
        if self.error.is_some() {
            let text = if self.error_local { "Can't save" } else { "Can't reach node" };
            return Chip { look: Look::Offline, text: text.into(), label: failed_title(self).into() };
        }
        if self.synced {
            return Chip { look: Look::Synced, text: "Synced".into(), label: "Synced".into() };
        }
        let percent = percent(self);
        match self.seconds_left {
            Some(s) => Chip {
                look: Look::Ring,
                text: format!("{} left", short_time(s)),
                label: format!("Catching up, {percent}, about {} left", long_time(s)),
            },
            // Measured, but the time left dropped for a slowdown: no number.
            None if self.blocks_per_second.is_some() => Chip { look: Look::Ring, text: "Catching up".into(), label: format!("Catching up, {percent}") },
            None => Chip { look: Look::Ring, text: "Starting".into(), label: format!("Catching up, {percent}") },
        }
    }
}

/// One of the card's rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// "Blocks".
    pub label: String,
    /// "3,360 of 9,880 · 28/s".
    pub value: String,
    /// What TalkBack reads: "Blocks: 3,360 of 9,880, 28 a second".
    pub spoken: String,
}

/// The card the chip opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    /// "Catching up", "Synced", "Can't reach node", "Can't save the wallet".
    pub title: String,
    /// "34%" beside the title while catching up.
    pub percent: Option<String>,
    /// The bar, while catching up.
    pub progress: Option<f32>,
    /// Time left "about 4 min", Payments up to "29 Oct, 14:20", Blocks
    /// "3,360 of 9,880 · 28/s", Link speed "1.8 MB/s".
    pub rows: Vec<Row>,
    /// "You can still receive and send while Fob catches up."
    pub note: Option<String>,
    /// What went wrong, as the node or the network said it.
    pub error: Option<String>,
}

/// "Today, 14:20", "Yesterday, 14:20", "29 Oct, 14:20", "29 Oct 2025, 14:20".
fn date_time(time: u64, clock: Clock) -> String {
    let (day, seconds) = activity::local(time, clock);
    let (today, _) = activity::local(clock.now, clock);
    let date = match day {
        _ if day == today => "Today".to_owned(),
        _ if day == today - 1 => "Yesterday".to_owned(),
        _ => activity::date(day, today),
    };
    format!("{date}, {}", activity::time_of_day(seconds, clock))
}

/// The card's words for `status`.
pub fn details(status: &SyncStatus, can_spend: bool, clock: Clock) -> Details {
    let speeds = status.blocks_per_second.zip(status.link_mb_per_second);
    let catching_up = !status.synced && status.tip > 0;
    let row = |label: &str, value: String, spoken: String| Row { label: label.to_owned(), spoken: format!("{label}: {spoken}"), value };
    let mut rows = vec![];
    if catching_up && status.error.is_none() {
        let (left, spoken) = match status.seconds_left {
            Some(s) => (format!("about {}", short_time(s)), format!("about {}", long_time(s))),
            None => ("Working it out".to_owned(), "working it out".to_owned()),
        };
        rows.push(row("Time left", left, spoken));
    }
    if status.up_to > 0 {
        let when = date_time(status.up_to, clock);
        rows.push(row("Payments up to", when.clone(), when));
    }
    if catching_up {
        let blocks = format!("{} of {}", grouped(status.scanned.saturating_sub(status.start)), grouped(status.tip.saturating_sub(status.start)));
        let (value, spoken) = match speeds.filter(|_| status.error.is_none()) {
            Some((per, _)) => (format!("{blocks} · {}", per_second(per)), format!("{blocks}, {} a second", blocks_a_second(per))),
            None => (blocks.clone(), blocks),
        };
        rows.push(row("Blocks", value, spoken));
        if status.error.is_none() {
            let (value, spoken) = match speeds {
                Some((_, mb)) => (link_speed(mb * 1e6), link_speed_spoken(mb * 1e6)),
                None => ("Measuring".to_owned(), "measuring".to_owned()),
            };
            rows.push(row("Link speed", value, spoken));
        }
    } else if status.tip > 0 {
        let latest = grouped(status.tip - 1);
        rows.push(row("Latest block", latest.clone(), latest));
    }
    let title = match () {
        _ if status.error.is_some() => failed_title(status),
        _ if status.synced => "Synced",
        _ => "Catching up",
    };
    let note = match () {
        _ if status.error.is_some() => Some("Fob keeps trying on its own.".to_owned()),
        _ if status.synced => None,
        _ if can_spend => Some("You can still receive and send while Fob catches up.".to_owned()),
        _ => Some("You can still receive while Fob catches up.".to_owned()),
    };
    Details {
        title: title.to_owned(),
        percent: (!status.synced).then(|| percent(status)),
        progress: (!status.synced).then_some(status.progress.clamp(0.0, 1.0)),
        rows,
        note,
        error: status.error.clone(),
    }
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

    /// Block sizes vary along the chain: the speed over a few seconds wobbles
    /// by a fifth either way. The estimate, over a longer window, doesn't
    /// rise past the shown time enough to drop it, and the time counts down.
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

    /// A node that slows a lot: the number isn't raised, it's dropped
    /// ("Working it out"), and a fresh countdown starts once settled.
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

    /// After a failure the measuring starts again from the first report
    /// after the wait: no speed spans the wait, so the first time left
    /// isn't hours for a minute's backoff.
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
    fn times_are_rounded_for_reading() {
        let cases = [
            (1, "10 s", "10 seconds"),
            (11, "20 s", "20 seconds"),
            (55, "1 min", "1 minute"),
            (89, "1 min", "1 minute"),
            (90, "2 min", "2 minutes"),
            (370, "6 min", "6 minutes"),
            (3_590, "1 h", "1 hour"),
            (4_200, "1 h 10 min", "1 hour 10 minutes"),
            (7_300, "2 h", "2 hours"),
            (40_000, "11 h", "11 hours"),
        ];
        for (s, short, long) in cases {
            assert_eq!((short_time(s).as_str(), long_time(s).as_str()), (short, long), "{s}");
        }
        // Rounding never makes a smaller time read as more.
        let mut last = 0;
        for s in 0..50_000 {
            assert!(rounded(s) >= last, "{s}");
            last = rounded(s);
        }
    }

    #[test]
    fn link_and_block_speeds_read_plainly() {
        assert_eq!(link_speed(1_800_000.0), "1.8 MB/s");
        assert_eq!(link_speed(12_400_000.0), "12 MB/s");
        assert_eq!(link_speed(640_000.0), "640 kB/s");
        assert_eq!(per_second(28.2), "28/s");
        assert_eq!(per_second(0.84), "0.8/s");
        assert_eq!(per_second(1_234.0), "1,234/s");
    }

    fn catching_up() -> SyncStatus {
        SyncStatus { scanned: 1_003_360, tip: 1_009_880, start: 1_000_000, progress: 3_360.0 / 9_880.0, seconds_left: Some(240), up_to: NOW - 2 * 86_400, ..SyncStatus::default() }
    }

    const NOW: u64 = 1_793_000_000;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    #[test]
    fn the_chip_says_starting_then_the_time_left_then_synced() {
        let mut s = catching_up();
        s.seconds_left = None;
        let chip = s.chip();
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Ring, "Starting", "Catching up, 34%"));
        s.seconds_left = Some(370);
        let chip = s.chip();
        assert_eq!((chip.text.as_str(), chip.label.as_str()), ("6 min left", "Catching up, 34%, about 6 minutes left"));
        s.seconds_left = Some(17);
        assert_eq!(s.chip().text, "20 s left");
        // The time left dropped for a slowdown, the speed still known.
        (s.seconds_left, s.blocks_per_second) = (None, Some(4.0));
        assert_eq!((s.chip().text.as_str(), s.chip().label.as_str()), ("Catching up", "Catching up, 34%"));
        s.synced = true;
        assert_eq!((s.chip().look, s.chip().text.as_str()), (Look::Synced, "Synced"));
        s.error = Some("connection refused".into());
        assert_eq!((s.chip().look, s.chip().text.as_str()), (Look::Offline, "Can't reach node"));
    }

    #[test]
    fn nearly_done_still_reads_99_percent() {
        let s = SyncStatus { progress: 0.999, tip: 10, ..SyncStatus::default() };
        assert_eq!(s.chip().label, "Catching up, 99%");
    }

    #[test]
    fn the_card_while_catching_up_is_the_boards() {
        let s = SyncStatus { blocks_per_second: Some(28.0), link_mb_per_second: Some(1.8), ..catching_up() };
        let d = details(&s, true, clock());
        assert_eq!((d.title.as_str(), d.percent.as_deref()), ("Catching up", Some("34%")));
        let rows: Vec<(&str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
        assert_eq!(
            rows,
            [("Time left", "about 4 min"), ("Payments up to", "24 Oct, 07:33"), ("Blocks", "3,360 of 9,880 · 28/s"), ("Link speed", "1.8 MB/s")]
        );
        // TalkBack hears words, not "28 slash s" or "M B slash s".
        let spoken: Vec<&str> = d.rows.iter().map(|r| r.spoken.as_str()).collect();
        assert_eq!(
            spoken,
            ["Time left: about 4 minutes", "Payments up to: 24 Oct, 07:33", "Blocks: 3,360 of 9,880, 28 a second", "Link speed: 1.8 megabytes a second"]
        );
        let slow = SyncStatus { blocks_per_second: Some(0.84), link_mb_per_second: Some(0.64), ..catching_up() };
        let spoken: Vec<String> = details(&slow, true, clock()).rows.into_iter().map(|r| r.spoken).collect();
        assert_eq!(spoken[2..], ["Blocks: 3,360 of 9,880, 0.8 a second", "Link speed: 640 kilobytes a second"]);
        assert_eq!(d.note.as_deref(), Some("You can still receive and send while Fob catches up."));
        assert_eq!(details(&s, false, clock()).note.as_deref(), Some("You can still receive while Fob catches up."));
    }

    #[test]
    fn the_card_before_the_speed_settles_says_so() {
        let mut s = catching_up();
        s.seconds_left = None;
        let d = details(&s, true, clock());
        let rows: Vec<&str> = d.rows.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(rows, ["Working it out", "24 Oct, 07:33", "3,360 of 9,880", "Measuring"]);
        assert_eq!(d.rows[0].spoken, "Time left: working it out");
    }

    #[test]
    fn the_card_when_synced_or_offline() {
        let mut s = catching_up();
        (s.synced, s.scanned, s.progress, s.seconds_left, s.up_to) = (true, s.tip, 1.0, None, NOW - 90);
        let d = details(&s, true, clock());
        assert_eq!((d.title.as_str(), d.percent, d.progress, d.note), ("Synced", None, None, None));
        let rows: Vec<(&str, &str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str(), r.spoken.as_str())).collect();
        assert_eq!(rows, [("Payments up to", "Today, 07:31", "Payments up to: Today, 07:31"), ("Latest block", "1,009,879", "Latest block: 1,009,879")]);

        let mut s = catching_up();
        s.error = Some("node: connection refused".into());
        let d = details(&s, true, clock());
        assert_eq!(d.title, "Can't reach node");
        assert_eq!(d.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Payments up to", "Blocks"]);
        assert_eq!((d.note.as_deref(), d.error.as_deref()), (Some("Fob keeps trying on its own."), Some("node: connection refused")));

        // The phone's own failure doesn't blame the node.
        s.error = Some("Couldn't read or save the wallet: disk full".into());
        s.error_local = true;
        let d = details(&s, true, clock());
        assert_eq!((d.title.as_str(), d.note.as_deref()), ("Can't save the wallet", Some("Fob keeps trying on its own.")));
        let chip = s.chip();
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Offline, "Can't save", "Can't save the wallet"));
    }
}
