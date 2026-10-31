//! The wallet's coins: each output it owns, ready to spend or not yet. Money
//! not ready is listed piece by piece: each output that hasn't its 10
//! confirmations (a mined reward, its 60), where it came from, and about
//! when it will be ready. Home folds it into one line beside what's ready,
//! with a bar, and opens it into sums; the Coins screen lists every coin.

use crate::{
    activity::{Clock, local, short_when, time_of_day},
    amount, price, split,
};

/// About how long a block takes.
const BLOCK_SECONDS: u64 = 120;

/// Where a piece came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A payment to this wallet.
    Received,
    /// Change from one of this wallet's own payments, named as Activity
    /// titles it (the shop, or "86Ux … 7kQa"), when it's known.
    Change(Option<String>),
    /// One of a split's coins, or its change.
    Split,
}

impl Source {
    /// "Received · 08:40", "Received · 22 Oct", "Split up · 08:31",
    /// "Change · Corner Café", "Change".
    fn text(&self, time: u64, clock: Clock) -> String {
        match self {
            Source::Received => format!("Received · {}", short_when(time, clock)),
            Source::Split => format!("{} · {}", split::TITLE, short_when(time, clock)),
            Source::Change(Some(name)) => format!("Change · {name}"),
            Source::Change(None) => "Change".into(),
        }
    }
}

/// One output not ready yet, as the wallet knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub amount: u64,
    /// 0 while still in the pool or in a payment not yet mined.
    pub confirmations: u64,
    /// Confirmations it needs: 10, or 60 for a mined reward.
    pub needed: u64,
    pub source: Source,
    /// When it arrived (its block, or when it was seen), seconds since the epoch.
    pub time: u64,
}

/// One piece, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceView {
    /// In atomic units.
    pub atomic: u64,
    /// "0.653"
    pub amount: String,
    /// What it's worth now, when prices are on: "$108.44".
    pub fiat: Option<String>,
    pub confirmations: u64,
    pub needed: u64,
    /// `confirmations / needed`, for the ring.
    pub progress: f32,
    /// "Received · 08:40", "Change · Corner Café", "Split up · 08:31".
    pub source: String,
    /// "3 of 10"
    pub confirmations_text: String,
    /// When it should be ready: "09:02".
    pub ready: String,
    /// "in 14 min", "in 1 h 20 min".
    pub eta: String,
    /// Seconds since the epoch.
    pub ready_at: u64,
}

/// A line of Home's opened card: "Ready to spend  1.8592  $308.70".
#[derive(Clone, Debug, PartialEq)]
pub struct Sum {
    /// "Ready to spend", "Not ready · 3 coins", "All of it".
    pub label: String,
    /// In atomic units.
    pub atomic: u64,
    /// "1.8592"
    pub amount: String,
    /// What it's worth now, when prices are on: "$308.70".
    pub fiat: Option<String>,
}

impl Sum {
    /// For TalkBack, the line as one: "Ready to spend, 1.8592 XMR, $308.70".
    pub fn description(&self) -> String {
        [Some(self.label.clone()), Some(format!("{} XMR", self.amount)), self.fiat.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ")
    }
}

/// All the money not ready yet, beside what is, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct NotReady {
    /// Not ready, in atomic units.
    pub total: u64,
    /// Ready to spend, in atomic units.
    pub ready: u64,
    /// "1.8592 ready", the folded card's first words, in bold.
    pub ready_text: String,
    /// "0.6221 not yet", after them.
    pub not_yet_text: String,
    /// When the last of it should be ready: "09:02".
    pub all_ready: String,
    /// "All ready about 09:02 · in 14 min", under the opened card's sums.
    pub all_ready_line: String,
    /// The folded card for TalkBack: "1.8592 XMR ready to spend, 0.6221 XMR
    /// not yet, all ready about 09:02".
    pub description: String,
    /// Ready to spend, not ready, and all of it.
    pub sums: Vec<Sum>,
    /// How old the price the sums are worth at is, once it's missed a
    /// refresh: "23 h old".
    pub price_age: Option<String>,
    /// How far along, weighted by amount: Σ(amount × confirmations ÷ needed) ÷ total.
    pub progress: f32,
    /// Soonest ready first.
    pub pieces: Vec<PieceView>,
}

/// When the next block's two minutes count from: the last block, if it
/// came within the last two minutes; otherwise now (the wallet is behind,
/// or a block is late).
fn counting_from(last_block_time: u64, now: u64) -> u64 {
    if last_block_time <= now && now - last_block_time < BLOCK_SECONDS { last_block_time } else { now }
}

fn eta(seconds: u64) -> String {
    let minutes = seconds.div_ceil(60).max(1);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("in {m} min"),
        (h, 0) => format!("in {h} h"),
        (h, m) => format!("in {h} h {m} min"),
    }
}

fn clock_time(time: u64, clock: Clock) -> String {
    time_of_day(local(time, clock).1, clock)
}

/// "1 coin", "3 coins".
fn coins_text(n: usize) -> String {
    if n == 1 { "1 coin".into() } else { format!("{n} coins") }
}

/// The pieces as Home shows them beside the `ready` money, or `None` when
/// everything is ready.
pub fn summarise(pieces: Vec<Piece>, ready: u64, last_block_time: u64, clock: Clock) -> Option<NotReady> {
    let pieces: Vec<Piece> = pieces.into_iter().filter(|p| p.amount > 0 && p.confirmations < p.needed).collect();
    if pieces.is_empty() {
        return None;
    }
    let from = counting_from(last_block_time, clock.now);
    let total: u64 = pieces.iter().map(|p| p.amount).sum();
    let done: f64 = pieces.iter().map(|p| p.amount as f64 * p.confirmations as f64 / p.needed as f64).sum();
    let progress = (done / total as f64).clamp(0.0, 1.0);
    let mut views: Vec<PieceView> = pieces
        .into_iter()
        .map(|p| {
            let ready_at = from + (p.needed - p.confirmations) * BLOCK_SECONDS;
            PieceView {
                atomic: p.amount,
                amount: amount::format_short(p.amount),
                fiat: None,
                confirmations: p.confirmations,
                needed: p.needed,
                progress: p.confirmations as f32 / p.needed as f32,
                source: p.source.text(p.time, clock),
                confirmations_text: format!("{} of {}", p.confirmations, p.needed),
                ready: clock_time(ready_at, clock),
                eta: eta(ready_at.saturating_sub(clock.now)),
                ready_at,
            }
        })
        .collect();
    views.sort_by_key(|v| v.ready_at);
    let last = views.last().expect("a piece");
    let (all_ready, eta) = (last.ready.clone(), last.eta.clone());
    let (ready_text, total_text) = (amount::format_short(ready), amount::format_short(total));
    let sum = |label: String, atomic: u64| Sum { label, atomic, amount: amount::format_short(atomic), fiat: None };
    Some(NotReady {
        total,
        ready,
        description: format!("{ready_text} XMR ready to spend, {total_text} XMR not yet, all ready about {all_ready}"),
        ready_text: format!("{ready_text} ready"),
        not_yet_text: format!("{total_text} not yet"),
        all_ready_line: format!("All ready about {all_ready} · {eta}"),
        all_ready,
        sums: vec![
            sum("Ready to spend".into(), ready),
            sum(format!("Not ready · {}", coins_text(views.len())), total),
            sum("All of it".into(), ready + total),
        ],
        price_age: None,
        progress: progress as f32,
        pieces: views,
    })
}

/// A coin ready to spend, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct CoinView {
    /// In atomic units.
    pub atomic: u64,
    /// "1.2"
    pub amount: String,
    /// What it's worth now, when prices are on: "$199.27".
    pub fiat: Option<String>,
    /// "Received · 22 Oct", "Split up · 08:31", "Change · Ridge Hardware".
    pub source: String,
}

/// The Coins screen: every coin, ready or not, every word ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Coins {
    /// "Your 2.4813 XMR is held as 8 coins. A payment uses whole coins; …"
    pub intro: String,
    /// Ready to spend and not ready, in atomic units: the bar's two parts.
    pub ready: u64,
    pub not_ready: u64,
    /// The not-ready part's weighted progress, as on Home.
    pub progress: f32,
    /// The bar for TalkBack: "1.8592 XMR ready, 0.6221 XMR not yet, 37%
    /// of the way".
    pub bar_description: String,
    /// "Ready · 5 coins"
    pub ready_title: String,
    /// "1.8592 · $308.70", "1.8592" without a price.
    pub ready_sum: String,
    /// Largest first.
    pub ready_coins: Vec<CoinView>,
    /// "Not ready · 3 coins"
    pub not_ready_title: String,
    /// "all by 09:02"
    pub not_ready_sum: String,
    /// Soonest ready first.
    pub not_ready_coins: Vec<PieceView>,
    /// Split up can be pressed: some coin is ready to spend. Without one
    /// (no coins, or none ready yet) the button shows but is greyed out.
    pub can_split: bool,
}

/// Every coin as the Coins screen shows it: the `ready` ones and those not
/// ready yet, valued at `price` when there is one. A new coin, change
/// included, waits `unlock` blocks.
/// `age` says how old the price is once it's missed a refresh ("23 h old"),
/// beside the ready sum.
pub fn coins(ready: Vec<Piece>, not_ready: Vec<Piece>, last_block_time: u64, unlock: u64, price: Option<(&price::Currency, u64)>, age: Option<&str>, clock: Clock) -> Coins {
    let worth = |atomic: u64| price.map(|(currency, p)| price::value(atomic, p, currency));
    let ready_total: u64 = ready.iter().map(|p| p.amount).sum();
    let mut ready_coins: Vec<CoinView> = ready
        .into_iter()
        .filter(|p| p.amount > 0)
        .map(|p| CoinView { atomic: p.amount, amount: amount::format_short(p.amount), fiat: worth(p.amount), source: p.source.text(p.time, clock) })
        .collect();
    ready_coins.sort_by_key(|c| std::cmp::Reverse(c.atomic));
    let summary = summarise(not_ready, ready_total, last_block_time, clock);
    let (not_ready_total, progress, all_ready, mut pieces) = match summary {
        Some(n) => (n.total, n.progress, n.all_ready, n.pieces),
        None => (0, 0.0, String::new(), vec![]),
    };
    for piece in &mut pieces {
        piece.fiat = worth(piece.atomic);
    }
    let count = ready_coins.len() + pieces.len();
    let minutes = unlock * BLOCK_SECONDS / 60;
    let intro = if count == 0 {
        "No coins yet. Each payment to this wallet arrives as a coin.".to_owned()
    } else {
        format!(
            "Your {} XMR is held as {}. A payment uses whole coins; what's left comes back as a new coin, not ready for about {minutes} minutes.",
            amount::format_short(ready_total + not_ready_total),
            coins_text(count)
        )
    };
    let bar_description = match not_ready_total {
        0 => format!("{} XMR ready", amount::format_short(ready_total)),
        _ => format!(
            "{} XMR ready, {} XMR not yet, {}% of the way",
            amount::format_short(ready_total),
            amount::format_short(not_ready_total),
            (progress * 100.0).round() as u32
        ),
    };
    Coins {
        intro,
        ready: ready_total,
        not_ready: not_ready_total,
        progress,
        bar_description,
        ready_title: format!("Ready · {}", coins_text(ready_coins.len())),
        ready_sum: [Some(amount::format_short(ready_total)), worth(ready_total), worth(ready_total).and(age.map(str::to_owned))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ready_coins,
        not_ready_title: format!("Not ready · {}", coins_text(pieces.len())),
        not_ready_sum: if pieces.is_empty() { String::new() } else { format!("all by {all_ready}") },
        not_ready_coins: pieces,
        can_split: ready_total > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-31 08:48 UTC.
    const NOW: u64 = 1_793_448_000 - 3 * 3600 - 12 * 60;
    const XMR: u64 = amount::ATOMIC_PER_XMR;

    fn clock() -> Clock {
        Clock { now: NOW, utc_offset: 0, hours_24: true }
    }

    fn piece(amount: u64, confirmations: u64, source: Source, time: u64) -> Piece {
        Piece { amount, confirmations, needed: 10, source, time }
    }

    #[test]
    fn coins_read_like_the_designs() {
        let ready = |amount: u64, source: Source, time: u64| piece(amount, 10, source, time);
        let split = NOW - 17 * 60;
        let coins = coins(
            vec![
                ready(3 * XMR / 10, Source::Split, split),
                ready(XMR / 10, Source::Change(Some("Ridge Hardware".into())), NOW - 3600),
                ready(12 * XMR / 10, Source::Received, NOW - 9 * 86_400),
                ready(0, Source::Received, NOW),
                ready(XMR / 8, Source::Split, split),
            ],
            vec![piece(XMR / 2, 3, Source::Received, NOW - 8 * 60), piece(XMR / 50, 9, Source::Change(Some("84Bv … 9fQ2".into())), NOW - 20 * 60)],
            NOW - 30,
            10,
            Some((&price::currency("USD"), 166_060_000)),
            None,
            clock(),
        );
        assert_eq!(
            coins.intro,
            "Your 2.245 XMR is held as 6 coins. A payment uses whole coins; what's left comes back as a new coin, not ready for about 20 minutes."
        );
        assert_eq!((coins.ready_title.as_str(), coins.ready_sum.as_str()), ("Ready · 4 coins", "1.725 · $286.45"));
        let rows: Vec<_> = coins.ready_coins.iter().map(|c| (c.amount.as_str(), c.source.as_str(), c.fiat.as_deref())).collect();
        assert_eq!(
            rows,
            [
                ("1.2", "Received · 22 Oct", Some("$199.27")),
                ("0.3", "Split up · 08:31", Some("$49.82")),
                ("0.125", "Split up · 08:31", Some("$20.76")),
                ("0.1", "Change · Ridge Hardware", Some("$16.61")),
            ]
        );
        assert_eq!((coins.not_ready_title.as_str(), coins.not_ready_sum.as_str()), ("Not ready · 2 coins", "all by 09:01"));
        let rows: Vec<_> = coins.not_ready_coins.iter().map(|p| (p.amount.as_str(), p.source.as_str(), p.ready.as_str(), p.confirmations_text.as_str())).collect();
        assert_eq!(rows, [("0.02", "Change · 84Bv … 9fQ2", "08:49", "9 of 10"), ("0.5", "Received · 08:40", "09:01", "3 of 10")]);
        assert_eq!(coins.not_ready_coins[1].fiat.as_deref(), Some("$83.03"));
        assert_eq!((coins.ready, coins.not_ready), (1_725 * XMR / 1000, 52 * XMR / 100));
        // The bar for TalkBack says what it shows, not a bare percentage.
        assert_eq!(coins.bar_description, "1.725 XMR ready, 0.52 XMR not yet, 32% of the way");
        // An old price says its age beside the sum it values.
        let aged = super::coins(vec![piece(XMR, 10, Source::Received, NOW)], vec![], NOW, 10, Some((&price::currency("USD"), 166_060_000)), Some("23 h old"), clock());
        assert_eq!(aged.ready_sum, "1 · $166.06 · 23 h old");
        let all_ready = super::coins(vec![piece(XMR, 10, Source::Received, NOW)], vec![], NOW, 10, None, None, clock());
        assert_eq!(all_ready.bar_description, "1 XMR ready");
    }

    #[test]
    fn each_sum_reads_as_one_line() {
        let n = summarise(vec![piece(XMR / 2, 5, Source::Received, NOW)], 2 * XMR, NOW, clock()).unwrap();
        let read: Vec<String> = n.sums.iter().map(Sum::description).collect();
        assert_eq!(read, ["Ready to spend, 2 XMR", "Not ready · 1 coin, 0.5 XMR", "All of it, 2.5 XMR"]);
        let priced = Sum { fiat: Some("$308.70".into()), ..n.sums[0].clone() };
        assert_eq!(priced.description(), "Ready to spend, 2 XMR, $308.70");
    }

    #[test]
    fn no_coins_says_so() {
        let empty = coins(vec![], vec![], NOW, 10, None, None, clock());
        assert_eq!(empty.intro, "No coins yet. Each payment to this wallet arrives as a coin.");
        assert!(!empty.can_split);
        assert!(empty.ready_coins.is_empty() && empty.not_ready_coins.is_empty());
        assert_eq!((empty.ready_sum.as_str(), empty.not_ready_sum.as_str(), empty.progress), ("0", "", 0.0));
        // One coin, all ready, without a price.
        let one = coins(vec![piece(XMR, 12, Source::Received, NOW)], vec![], NOW, 10, None, None, clock());
        assert!(one.intro.starts_with("Your 1 XMR is held as 1 coin. "), "{}", one.intro);
        assert_eq!((one.ready_title.as_str(), one.ready_sum.as_str()), ("Ready · 1 coin", "1"));
        assert!(one.can_split);
        // A date from another year says the year.
        let old = coins(vec![piece(XMR, 12, Source::Received, NOW - 400 * 86_400)], vec![], NOW, 10, None, None, clock());
        assert_eq!(old.ready_coins[0].source, "Received · 26 Sep 2025");
    }

    #[test]
    fn split_up_waits_for_a_coin_ready_to_spend() {
        // Only coins not ready yet: Split up shows, greyed out.
        let waiting = coins(vec![], vec![piece(XMR, 3, Source::Received, NOW)], NOW, 10, None, None, clock());
        assert!(!waiting.can_split);
        // A ready coin of nothing doesn't count.
        let zero = coins(vec![piece(0, 10, Source::Received, NOW)], vec![], NOW, 10, None, None, clock());
        assert!(!zero.can_split);
        // One ready beside one waiting: it can be pressed.
        let both = coins(vec![piece(1, 10, Source::Received, NOW)], vec![piece(XMR, 3, Source::Received, NOW)], NOW, 10, None, None, clock());
        assert!(both.can_split);
    }

    #[test]
    fn everything_ready_shows_nothing() {
        assert_eq!(summarise(vec![], XMR, NOW, clock()), None);
        assert_eq!(summarise(vec![piece(XMR, 10, Source::Received, NOW)], XMR, NOW, clock()), None);
    }

    #[test]
    fn pieces_read_like_the_designs() {
        // The last block came 30 s ago.
        let last = NOW - 30;
        let pieces = vec![
            piece(653 * XMR / 1000, 3, Source::Received, NOW - 8 * 60),
            piece(XMR / 2, 0, Source::Change(Some("Corner Café".into())), NOW - 60),
            piece(XMR / 10, 9, Source::Change(None), NOW - 20 * 60),
        ];
        let n = summarise(pieces, 18_592 * XMR / 10_000, last, clock()).unwrap();
        assert_eq!((n.ready_text.as_str(), n.not_yet_text.as_str()), ("1.8592 ready", "1.253 not yet"));
        // 07:48 + 30 s is 08:47:30; ten blocks on is 09:07:30.
        assert_eq!((n.all_ready.as_str(), n.all_ready_line.as_str()), ("09:07", "All ready about 09:07 · in 20 min"));
        assert_eq!(n.description, "1.8592 XMR ready to spend, 1.253 XMR not yet, all ready about 09:07");
        let sums: Vec<_> = n.sums.iter().map(|s| (s.label.as_str(), s.amount.as_str())).collect();
        assert_eq!(sums, [("Ready to spend", "1.8592"), ("Not ready · 3 coins", "1.253"), ("All of it", "3.1122")]);
        assert_eq!((n.ready, n.total), (18_592 * XMR / 10_000, 1_253 * XMR / 1000));
        // (0.653 × 0.3 + 0.1 × 0.9) ÷ 1.253 = 22.8%.
        assert!((n.progress - 0.2282).abs() < 0.001);
        let rows: Vec<_> = n.pieces.iter().map(|p| (p.amount.as_str(), p.source.as_str(), p.ready.as_str(), p.eta.as_str(), p.confirmations)).collect();
        assert_eq!(
            rows,
            [
                ("0.1", "Change", "08:49", "in 2 min", 9),
                ("0.653", "Received · 08:40", "09:01", "in 14 min", 3),
                ("0.5", "Change · Corner Café", "09:07", "in 20 min", 0),
            ]
        );
    }

    #[test]
    fn one_piece_says_so() {
        // Nothing ready yet: the sums still read.
        let n = summarise(vec![piece(XMR, 5, Source::Received, NOW)], 0, NOW, clock()).unwrap();
        assert_eq!((n.ready_text.as_str(), n.sums[1].label.as_str()), ("0 ready", "Not ready · 1 coin"));
        assert_eq!(n.all_ready_line, "All ready about 08:58 · in 10 min");
        assert_eq!((n.progress, n.pieces[0].progress), (0.5, 0.5));
    }

    #[test]
    fn an_old_last_block_counts_from_now() {
        // Catching up: the last block scanned is a day old.
        let n = summarise(vec![piece(XMR, 9, Source::Received, NOW)], 0, NOW - 86_400, clock()).unwrap();
        assert_eq!((n.pieces[0].ready_at, n.pieces[0].eta.as_str()), (NOW + 120, "in 2 min"));
        // A last block from the future (the phone's clock is behind) does too.
        assert_eq!(summarise(vec![piece(XMR, 9, Source::Received, NOW)], 0, NOW + 50, clock()).unwrap().pieces[0].ready_at, NOW + 120);
    }

    #[test]
    fn a_mined_reward_waits_sixty_blocks() {
        let reward = Piece { needed: 60, ..piece(XMR, 20, Source::Received, NOW) };
        let n = summarise(vec![reward], 0, NOW, clock()).unwrap();
        assert_eq!(n.pieces[0].eta, "in 1 h 20 min");
        assert!((n.progress - 1.0 / 3.0).abs() < 0.001);
        assert_eq!(eta(7200), "in 2 h");
        assert_eq!(eta(0), "in 1 min");
    }
}
