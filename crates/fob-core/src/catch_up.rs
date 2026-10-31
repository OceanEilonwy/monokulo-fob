//! How catching up looks: the chip in Home's top bar and the details card
//! under it. Speeds come from a short window of what the sync loop saw
//! (blocks scanned, bytes the fetcher took from the node), and the time
//! left from them, settled and rounded so it reads calmly.

use std::{collections::VecDeque, time::Duration, time::Instant};

use crate::{
    activity::{self, Clock},
    price::grouped,
    wallet::SyncStatus,
};

/// Speeds are measured over about this much of the latest samples.
const SPAN: Duration = Duration::from_secs(5);
/// No speed, so no time left, until this much has been measured.
const SETTLE: Duration = Duration::from_secs(3);
/// A first time left under this isn't shown: a catch-up that's nearly over
/// shows only Starting, then Synced.
const FIRST_SHOWN: u64 = 10;
/// The time left goes up only when a new estimate is at least half as much
/// again and this many seconds more.
const RISE: u64 = 30;

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
    shown: Option<u64>,
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
        // Keep one sample from before the window, as its base.
        while self.samples.len() > 2 && self.samples[1].at + SPAN <= at {
            self.samples.pop_front();
        }
        self.shown = self.next_shown(left);
        self.shown
    }

    /// Speeds over the window, once [`SETTLE`] has been measured.
    pub fn speeds(&self) -> Option<Speeds> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        if last.at.duration_since(self.started?) < SETTLE {
            return None;
        }
        let seconds = last.at.duration_since(first.at).as_secs_f64();
        (seconds > 0.0).then(|| Speeds {
            blocks_per_second: (last.blocks - first.blocks) as f64 / seconds,
            bytes_per_second: (last.bytes - first.bytes) as f64 / seconds,
        })
    }

    /// The time left as shown, in seconds: none until the speed settles, and
    /// then never up again unless the estimate rises a lot.
    fn next_shown(&self, left: u64) -> Option<u64> {
        if left == 0 {
            return None;
        }
        let speeds = self.speeds()?;
        if speeds.blocks_per_second <= 0.0 {
            // Stalled: keep what was shown rather than guess.
            return self.shown;
        }
        let estimate = (left as f64 / speeds.blocks_per_second).ceil() as u64;
        match self.shown {
            None if estimate < FIRST_SHOWN => None,
            None => Some(estimate),
            Some(shown) if estimate <= shown => Some(estimate),
            Some(shown) if estimate >= shown + shown / 2 && estimate >= shown + RISE => Some(estimate),
            Some(shown) => Some(shown),
        }
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

/// "28/s", or "0.8/s" for slow going.
fn per_second(blocks: f64) -> String {
    if blocks >= 9.95 { format!("{}/s", grouped(blocks.round() as u64)) } else { format!("{blocks:.1}/s") }
}

/// "1.8 MB/s", "12 MB/s", or "640 kB/s" under a megabyte (decimal units).
pub fn link_speed(bytes_per_second: f64) -> String {
    let mb = bytes_per_second / 1e6;
    match () {
        _ if mb >= 9.95 => format!("{} MB/s", mb.round()),
        _ if mb >= 0.995 => format!("{mb:.1} MB/s"),
        _ => format!("{} kB/s", (bytes_per_second / 1e3).round()),
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
    /// "Starting", "6 min left", "Synced", "Can't reach node".
    pub text: String,
    /// What TalkBack reads: "Catching up, 34%, about 6 minutes left".
    pub label: String,
}

/// Percent done, never 100 before the end: "34%".
fn percent(status: &SyncStatus) -> String {
    let p = (status.progress.clamp(0.0, 1.0) * 100.0).floor() as u32;
    format!("{}%", if status.synced { p } else { p.min(99) })
}

impl SyncStatus {
    pub fn chip(&self) -> Chip {
        if self.error.is_some() {
            let text = "Can't reach node".to_owned();
            return Chip { look: Look::Offline, label: text.clone(), text };
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
            None => Chip { look: Look::Ring, text: "Starting".into(), label: format!("Catching up, {percent}") },
        }
    }
}

/// The card the chip opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    /// "Catching up", "Synced", "Can't reach node".
    pub title: String,
    /// "34%" beside the title while catching up.
    pub percent: Option<String>,
    /// The bar, while catching up.
    pub progress: Option<f32>,
    /// ("Time left", "about 4 min"), ("Payments up to", "29 Oct, 14:20"),
    /// ("Blocks", "3,360 of 9,880 · 28/s"), ("Link speed", "1.8 MB/s").
    pub rows: Vec<(String, String)>,
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
    let row = |k: &str, v: String| (k.to_owned(), v);
    let mut rows = vec![];
    if catching_up && status.error.is_none() {
        let left = status.seconds_left.map_or_else(|| "Working it out".to_owned(), |s| format!("about {}", short_time(s)));
        rows.push(row("Time left", left));
    }
    if status.up_to > 0 {
        rows.push(row("Payments up to", date_time(status.up_to, clock)));
    }
    if catching_up {
        let mut blocks = format!("{} of {}", grouped(status.scanned.saturating_sub(status.start)), grouped(status.tip.saturating_sub(status.start)));
        if let Some((per, _)) = speeds.filter(|_| status.error.is_none()) {
            blocks = format!("{blocks} · {}", per_second(per));
        }
        rows.push(row("Blocks", blocks));
        if status.error.is_none() {
            rows.push(row("Link speed", speeds.map_or_else(|| "Measuring".to_owned(), |(_, mb)| link_speed(mb * 1e6))));
        }
    } else if status.tip > 0 {
        rows.push(row("Latest block", grouped(status.tip - 1)));
    }
    let title = match () {
        _ if status.error.is_some() => "Can't reach node",
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

    #[test]
    fn the_time_left_counts_down_and_never_up_for_a_wobble() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        // 10,000 blocks at 20/s: about 500 s.
        let (shown, at) = run(&mut pace, t0, (0.0, 4.0), (20.0, 1e6), (0, 0), 10_000);
        let first = shown.unwrap();
        assert!((480..=500).contains(&first), "{first}");
        // A bit slower (16/s, an estimate about a quarter higher): held.
        let (shown, at) = run(&mut pace, t0, (4.5, 12.0), (16.0, 1e6), at, 10_000);
        assert_eq!(shown, Some(first));
        // Back to 20/s: it counts down again from there.
        let (shown, _) = run(&mut pace, t0, (12.5, 40.0), (20.0, 1e6), at, 10_000);
        assert!(shown.unwrap() < first);
    }

    #[test]
    fn the_time_left_goes_up_when_the_estimate_rises_a_lot() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (shown, at) = run(&mut pace, t0, (0.0, 4.0), (40.0, 1e6), (0, 0), 10_000);
        let first = shown.unwrap();
        // A quarter of the speed: four times as long, shown once the window sees it.
        let (shown, _) = run(&mut pace, t0, (4.5, 12.0), (10.0, 1e6), at, 10_000);
        assert!(shown.unwrap() > first * 3, "{shown:?} after {first}");
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
    fn a_stall_keeps_what_was_shown() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        let (shown, at) = run(&mut pace, t0, (0.0, 4.0), (20.0, 1e6), (0, 0), 10_000);
        // Nothing at all for the whole window: no guess.
        let (received, scanned) = (at.0 - 500_000, at.1 - 10);
        assert_eq!(pace.sample(t0 + secs(20.0), received, scanned, 10_000 - scanned), shown);
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
        let rows: Vec<(&str, &str)> = d.rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            rows,
            [("Time left", "about 4 min"), ("Payments up to", "24 Oct, 07:33"), ("Blocks", "3,360 of 9,880 · 28/s"), ("Link speed", "1.8 MB/s")]
        );
        assert_eq!(d.note.as_deref(), Some("You can still receive and send while Fob catches up."));
        assert_eq!(details(&s, false, clock()).note.as_deref(), Some("You can still receive while Fob catches up."));
    }

    #[test]
    fn the_card_before_the_speed_settles_says_so() {
        let mut s = catching_up();
        s.seconds_left = None;
        let d = details(&s, true, clock());
        let rows: Vec<&str> = d.rows.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(rows, ["Working it out", "24 Oct, 07:33", "3,360 of 9,880", "Measuring"]);
    }

    #[test]
    fn the_card_when_synced_or_offline() {
        let mut s = catching_up();
        (s.synced, s.scanned, s.progress, s.seconds_left, s.up_to) = (true, s.tip, 1.0, None, NOW - 90);
        let d = details(&s, true, clock());
        assert_eq!((d.title.as_str(), d.percent, d.progress, d.note), ("Synced", None, None, None));
        assert_eq!(d.rows, [("Payments up to".to_owned(), "Today, 07:31".to_owned()), ("Latest block".to_owned(), "1,009,879".to_owned())]);

        let mut s = catching_up();
        s.error = Some("node: connection refused".into());
        let d = details(&s, true, clock());
        assert_eq!(d.title, "Can't reach node");
        assert_eq!(d.rows.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["Payments up to", "Blocks"]);
        assert_eq!((d.note.as_deref(), d.error.as_deref()), (Some("Fob keeps trying on its own."), Some("node: connection refused")));
    }
}
