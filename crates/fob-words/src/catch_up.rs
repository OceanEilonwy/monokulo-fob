//! How catching up looks, in words.

use fob_monero::{amount::grouped, network::{ZmqStatus, link_amount, link_speed}};
use fob_screens::{catch_up::{self, Look, Note, SyncState, Title, rounded}, clock::When};

use crate::clock;

/// Settings' node line while it's failing, and while it's answering.
pub const NODE_FAILING: &str = "Can't reach node";
pub const NODE_WORKING: &str = "Working";

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

/// "40 s", "2 min": how long the node has kept the wallet waiting, unrounded
/// below a minute.
fn waited(seconds: u64) -> String {
    if seconds < 60 { format!("{seconds} s") } else { format!("{} min", seconds / 60) }
}

/// "28", or "0.8" for slow going.
fn blocks_a_second(blocks: f64) -> String {
    if blocks >= 9.95 { grouped(blocks.round() as u64) } else { format!("{blocks:.1}") }
}

/// "28/s", or "0.8/s" for slow going.
fn per_second(blocks: f64) -> String {
    format!("{}/s", blocks_a_second(blocks))
}

/// The same for TalkBack: "1.8 megabytes a second".
fn link_speed_spoken(bytes_per_second: f64) -> String {
    let (amount, unit) = link_amount(bytes_per_second);
    format!("{amount} {} a second", if unit == "MB" { "megabytes" } else { "kilobytes" })
}

/// The chip in Home's top bar, as it's said.
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
fn failed_title(local: bool) -> &'static str {
    if local { "Can't save the wallet" } else { "Can't reach node" }
}

pub fn chip(c: catch_up::Chip) -> Chip {
    let (text, label) = match c.state {
        SyncState::Failed { local } => (if local { "Can't save" } else { "Can't reach node" }.to_owned(), failed_title(local).to_owned()),
        SyncState::ChainChanged => ("Can't sync".to_owned(), "Can't sync".to_owned()),
        SyncState::Connecting => ("Connecting".to_owned(), "Connecting to the node".to_owned()),
        SyncState::Synced => ("Synced".to_owned(), "Synced".to_owned()),
        SyncState::Waiting { percent } => ("Waiting for node".to_owned(), format!("Catching up, {percent}%, waiting for the node")),
        SyncState::Left { seconds, percent } => (format!("{} left", short_time(seconds)), format!("Catching up, {percent}%, about {} left", long_time(seconds))),
        SyncState::CatchingUp { percent } => ("Catching up".to_owned(), format!("Catching up, {percent}%")),
        SyncState::Starting { percent } => ("Starting".to_owned(), format!("Catching up, {percent}%")),
    };
    Chip { look: c.look, text, label }
}

/// One of the card's rows, as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// "Blocks".
    pub label: String,
    /// "3,360 of 9,880 · 28/s".
    pub value: String,
    /// What TalkBack reads: "Blocks: 3,360 of 9,880, 28 a second".
    pub spoken: String,
}

/// "Today, 14:20", "Yesterday, 14:20", "29 Oct, 14:20", "29 Oct 2025, 14:20".
pub fn date_time(when: &When) -> String {
    format!("{}, {}", clock::day(&when.day), when.time)
}

fn row(row: catch_up::Row) -> Row {
    let (label, value, spoken) = match row {
        catch_up::Row::TimeLeft(Some(s)) => ("Time left", format!("about {}", short_time(s)), format!("about {}", long_time(s))),
        catch_up::Row::TimeLeft(None) => ("Time left", "Working it out".to_owned(), "working it out".to_owned()),
        catch_up::Row::PaymentsUpTo(when) => ("Payments up to", date_time(&when), date_time(&when)),
        catch_up::Row::Blocks { done, of, per_second: per } => {
            let blocks = format!("{} of {}", grouped(done), grouped(of));
            match per {
                Some(per) => ("Blocks", format!("{blocks} · {}", per_second(per)), format!("{blocks}, {} a second", blocks_a_second(per))),
                None => ("Blocks", blocks.clone(), blocks),
            }
        }
        catch_up::Row::LinkSpeed(Some(bytes)) => ("Link speed", link_speed(bytes), link_speed_spoken(bytes)),
        catch_up::Row::LinkSpeed(None) => ("Link speed", "Measuring".to_owned(), "measuring".to_owned()),
        catch_up::Row::LatestBlock(block) => ("Latest block", grouped(block), grouped(block)),
    };
    Row { label: label.to_owned(), spoken: format!("{label}: {spoken}"), value }
}

/// The card the chip opens, as it's said.
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
    /// "Fob keeps trying on its own." while it's failing.
    pub note: Option<String>,
    /// What went wrong, as the node or the network said it.
    pub error: Option<String>,
    /// "Scan the whole wallet again", when that's the way past the failure.
    pub scan_again: Option<String>,
}

pub fn details(d: catch_up::Details) -> Details {
    Details {
        title: match d.title {
            Title::Failed { local } => failed_title(local),
            Title::ChainChanged => "Can't sync",
            Title::Connecting => "Connecting",
            Title::Synced => "Synced",
            Title::Waiting => "Waiting for node",
            Title::CatchingUp => "Catching up",
        }
        .to_owned(),
        percent: d.percent.map(|p| format!("{p}%")),
        progress: d.progress,
        rows: d.rows.into_iter().map(row).collect(),
        note: d.note.map(|note| match note {
            Note::KeepsTrying => "Fob keeps trying on its own.".to_owned(),
            Note::WrongChain => "Your node may be on the wrong chain, so Fob keeps checking. If the chain really did change, scan again to put your balance right.".to_owned(),
            Note::AskingHowFar => "Asking the node how far the chain has got.".to_owned(),
            Note::NoBlocksFor { seconds } => format!("The node hasn't sent any blocks for {}. Fob keeps asking, and another node may be quicker.", waited(seconds)),
        }),
        error: d.error,
        scan_again: d.scan_again.then(|| "Scan the whole wallet again".to_owned()),
    }
}

/// The subtitle under "Instant updates (ZMQ)":
/// "node.example:18083 · listening", or "Off".
pub fn zmq_text(status: &ZmqStatus, endpoint: Option<&str>) -> String {
    let word = match status {
        ZmqStatus::Off => return "Off".to_owned(),
        ZmqStatus::Connecting => "connecting",
        ZmqStatus::Listening => "listening",
        ZmqStatus::CantConnect(_) => "can't connect",
    };
    let place = endpoint.map(|e| e.trim_start_matches("tcp://")).unwrap_or("");
    format!("{place} · {word}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_screens::{catch_up::{SyncStatus, WAIT_SHOWN}, clock::Clock};

    fn details(status: &SyncStatus, clock: Clock) -> Details {
        super::details(catch_up::details(status, clock))
    }

    fn said_chip(status: &SyncStatus) -> Chip {
        chip(status.chip())
    }

    fn catching_up() -> SyncStatus {
        SyncStatus { connected: true, scanned: 1_003_360, tip: 1_009_880, start: 1_000_000, progress: 3_360.0 / 9_880.0, seconds_left: Some(240), up_to: NOW - 2 * 86_400, ..SyncStatus::default() }
    }

    const NOW: u64 = 1_793_000_000;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }


    #[test]
    fn the_subtitle_names_the_place_and_how_it_is_doing() {
        let at = Some("tcp://node.example:18083");
        assert_eq!(zmq_text(&ZmqStatus::Off, at), "Off");
        assert_eq!(zmq_text(&ZmqStatus::Listening, at), "node.example:18083 · listening");
        assert_eq!(zmq_text(&ZmqStatus::CantConnect("no answer".into()), at), "node.example:18083 · can't connect");
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

    #[test]
    fn the_chip_says_starting_then_the_time_left_then_synced() {
        let mut s = catching_up();
        s.seconds_left = None;
        let chip = said_chip(&s);
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Ring, "Starting", "Catching up, 34%"));
        s.seconds_left = Some(370);
        let chip = said_chip(&s);
        assert_eq!((chip.text.as_str(), chip.label.as_str()), ("6 min left", "Catching up, 34%, about 6 minutes left"));
        s.seconds_left = Some(17);
        assert_eq!(said_chip(&s).text, "20 s left");
        // The time left dropped for a slowdown, the speed still known.
        (s.seconds_left, s.blocks_per_second) = (None, Some(4.0));
        assert_eq!((said_chip(&s).text.as_str(), said_chip(&s).label.as_str()), ("Catching up", "Catching up, 34%"));
        s.synced = true;
        assert_eq!((said_chip(&s).look, said_chip(&s).text.as_str()), (Look::Synced, "Synced"));
        s.error = Some("connection refused".into());
        assert_eq!((said_chip(&s).look, said_chip(&s).text.as_str()), (Look::Offline, "Can't reach node"));
    }

    /// Back on screen with the wallet caught up as of last time, or just on
    /// another node: it isn't Synced until the node has said so.
    #[test]
    fn synced_from_last_time_waits_for_the_node_to_answer() {
        let mut s = catching_up();
        (s.synced, s.scanned, s.progress, s.connected) = (true, s.tip, 1.0, false);
        let chip = said_chip(&s);
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Ring, "Connecting", "Connecting to the node"));
        let d = details(&s, clock());
        assert_eq!((d.title.as_str(), d.percent, d.progress), ("Connecting", None, None));
        assert_eq!(d.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Payments up to"]);
        assert_eq!(d.note.as_deref(), Some("Asking the node how far the chain has got."));
        s.connected = true;
        assert_eq!(said_chip(&s).text, "Synced");
    }

    /// A node that answers but sends nothing says so, in place of a time left
    /// counting down on nothing.
    #[test]
    fn a_node_keeping_it_waiting_is_named() {
        let mut s = catching_up();
        s.waiting_seconds = WAIT_SHOWN - 1;
        assert_eq!(said_chip(&s).text, "4 min left");
        s.waiting_seconds = 40;
        let chip = said_chip(&s);
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Ring, "Waiting for node", "Catching up, 34%, waiting for the node"));
        let d = details(&s, clock());
        assert_eq!((d.title.as_str(), d.percent.as_deref()), ("Waiting for node", Some("34%")));
        assert_eq!(d.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Payments up to", "Blocks"]);
        assert_eq!(d.note.as_deref(), Some("The node hasn't sent any blocks for 40 s. Fob keeps asking, and another node may be quicker."));
        s.waiting_seconds = 130;
        assert!(details(&s, clock()).note.unwrap().contains("for 2 min."));
    }

    #[test]
    fn nearly_done_still_reads_99_percent() {
        let s = SyncStatus { connected: true, progress: 0.999, tip: 10, ..SyncStatus::default() };
        assert_eq!(said_chip(&s).label, "Catching up, 99%");
    }

    #[test]
    fn the_card_while_catching_up_is_the_boards() {
        let s = SyncStatus { blocks_per_second: Some(28.0), link_mb_per_second: Some(1.8), ..catching_up() };
        let d = details(&s, clock());
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
        let spoken: Vec<String> = details(&slow, clock()).rows.into_iter().map(|r| r.spoken).collect();
        assert_eq!(spoken[2..], ["Blocks: 3,360 of 9,880, 0.8 a second", "Link speed: 640 kilobytes a second"]);
        // Catching up isn't news: no note under the rows.
        assert_eq!(d.note, None);
    }

    #[test]
    fn the_card_before_the_speed_settles_says_so() {
        let mut s = catching_up();
        s.seconds_left = None;
        let d = details(&s, clock());
        let rows: Vec<&str> = d.rows.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(rows, ["Working it out", "24 Oct, 07:33", "3,360 of 9,880", "Measuring"]);
        assert_eq!(d.rows[0].spoken, "Time left: working it out");
    }

    #[test]
    fn the_card_when_synced_or_offline() {
        let mut s = catching_up();
        (s.synced, s.scanned, s.progress, s.seconds_left, s.up_to) = (true, s.tip, 1.0, None, NOW - 90);
        let d = details(&s, clock());
        assert_eq!((d.title.as_str(), d.percent, d.progress, d.note), ("Synced", None, None, None));
        let rows: Vec<(&str, &str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str(), r.spoken.as_str())).collect();
        assert_eq!(rows, [("Payments up to", "Today, 07:31", "Payments up to: Today, 07:31"), ("Latest block", "1,009,879", "Latest block: 1,009,879")]);

        let mut s = catching_up();
        s.error = Some("node: connection refused".into());
        let d = details(&s, clock());
        assert_eq!(d.title, "Can't reach node");
        assert_eq!(d.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Payments up to", "Blocks"]);
        assert_eq!((d.note.as_deref(), d.error.as_deref()), (Some("Fob keeps trying on its own."), Some("node: connection refused")));

        // The phone's own failure doesn't blame the node.
        s.error = Some("Couldn't read or save the wallet: disk full".into());
        s.error_local = true;
        let d = details(&s, clock());
        assert_eq!((d.title.as_str(), d.note.as_deref()), ("Can't save the wallet", Some("Fob keeps trying on its own.")));
        let chip = said_chip(&s);
        assert_eq!((chip.look, chip.text.as_str(), chip.label.as_str()), (Look::Offline, "Can't save", "Can't save the wallet"));
        assert_eq!(d.scan_again, None);

        // A chain that changed too far back says so, and how to get past it.
        s.error_local = false;
        s.chain_changed = true;
        s.error = Some("The chain changed further back than Fob can check".into());
        let d = details(&s, clock());
        assert_eq!((d.title.as_str(), d.scan_again.as_deref()), ("Can't sync", Some("Scan the whole wallet again")));
        assert!(d.note.unwrap().contains("wrong chain"));
        let said = said_chip(&s);
        assert_eq!((said.look, said.text.as_str()), (Look::Offline, "Can't sync"));
    }
}
