//! Coins, ready to spend and not yet, in words.

use fob_monero::amount;
use fob_screens::not_ready::{self, Source, SumKind};

use crate::{prices, split};

/// "Received · 08:40", "Received · 22 Oct", "Split up · 08:31",
/// "Change · Corner Café", "Change".
fn source(source: &Source, came: &str) -> String {
    match source {
        Source::Received => format!("Received · {came}"),
        Source::Split => format!("{} · {came}", split::TITLE),
        Source::Change(Some(name)) => format!("Change · {name}"),
        Source::Change(None) => "Change".into(),
    }
}

/// "in 14 min", "in 1 h 20 min".
fn eta(seconds: u64) -> String {
    let minutes = seconds.div_ceil(60).max(1);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("in {m} min"),
        (h, 0) => format!("in {h} h"),
        (h, m) => format!("in {h} h {m} min"),
    }
}

/// One piece as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceView {
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
    pub ready_at: u64,
}

pub fn piece(p: not_ready::PieceView) -> PieceView {
    PieceView {
        atomic: p.atomic,
        amount: p.amount,
        fiat: p.fiat,
        confirmations: p.confirmations,
        needed: p.needed,
        progress: p.progress,
        source: source(&p.source, &p.came),
        confirmations_text: format!("{} of {}", p.confirmations, p.needed),
        ready: p.ready,
        eta: eta(p.seconds_left),
        ready_at: p.ready_at,
    }
}

/// A line of Home's opened card as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct Sum {
    /// "Ready to spend", "Not ready · 3 coins", "All of it".
    pub label: String,
    pub atomic: u64,
    /// "1.8592"
    pub amount: String,
    /// What it's worth now, when prices are on: "$308.70".
    pub fiat: Option<String>,
    /// For TalkBack, the line as one: "Ready to spend, 1.8592 XMR, $308.70".
    pub description: String,
}

pub fn sum(s: not_ready::Sum) -> Sum {
    let label = match s.kind {
        SumKind::Ready => "Ready to spend".to_owned(),
        SumKind::NotReady { coins } => format!("Not ready · {}", split::coins(coins as u32)),
        SumKind::All => "All of it".to_owned(),
    };
    let description = [Some(label.clone()), Some(format!("{} XMR", s.amount)), s.fiat.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ");
    Sum { label, atomic: s.atomic, amount: s.amount, fiat: s.fiat, description }
}

/// All the money not ready yet, as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct NotReady {
    /// Not ready, in atomic units.
    pub not_ready: u64,
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
    /// How old the price the sums are worth at is: "23 h old".
    pub price_age: Option<String>,
    pub progress: f32,
    /// Soonest ready first.
    pub pieces: Vec<PieceView>,
}

pub fn not_ready(n: not_ready::NotReady) -> NotReady {
    let (ready_text, total_text) = (amount::format_short(n.ready), amount::format_short(n.total));
    let price_age = n.price_age.map(prices::age);
    let line = format!("All ready about {} · {}", n.all_ready, eta(n.seconds_left));
    NotReady {
        not_ready: n.total,
        ready: n.ready,
        description: format!("{ready_text} XMR ready to spend, {total_text} XMR not yet, all ready about {}", n.all_ready),
        ready_text: format!("{ready_text} ready"),
        not_yet_text: format!("{total_text} not yet"),
        all_ready_line: match &price_age {
            Some(age) => format!("{line} · price {age}"),
            None => line,
        },
        all_ready: n.all_ready,
        sums: n.sums.into_iter().map(sum).collect(),
        price_age,
        progress: n.progress,
        pieces: n.pieces.into_iter().map(piece).collect(),
    }
}

/// A coin ready to spend, as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct CoinView {
    pub atomic: u64,
    /// "1.2"
    pub amount: String,
    /// What it's worth now, when prices are on: "$199.27".
    pub fiat: Option<String>,
    /// "Received · 22 Oct", "Split up · 08:31", "Change · Ridge Hardware".
    pub source: String,
}

/// The Coins screen as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct Coins {
    /// "Your 2.4813 XMR is held as 8 coins. A payment uses whole coins; …"
    pub intro: String,
    pub ready: u64,
    pub not_ready: u64,
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
    pub can_split: bool,
}

pub fn coins(c: not_ready::Coins) -> Coins {
    let intro = match &c.held {
        None => "No coins yet. Each payment to this wallet arrives as a coin.".to_owned(),
        Some(held) => format!(
            "Your {} XMR is held as {}. A payment uses whole coins; what's left comes back as a new coin, not ready for about {} minutes.",
            held.amount,
            split::coins(held.coins as u32),
            held.minutes
        ),
    };
    let bar_description = match c.not_ready {
        0 => format!("{} XMR ready", c.ready_amount),
        _ => format!("{} XMR ready, {} XMR not yet, {}% of the way", c.ready_amount, amount::format_short(c.not_ready), (c.progress * 100.0).round() as u32),
    };
    Coins {
        intro,
        ready: c.ready,
        not_ready: c.not_ready,
        progress: c.progress,
        bar_description,
        ready_title: format!("Ready · {}", split::coins(c.ready_coins.len() as u32)),
        ready_sum: [Some(c.ready_amount), c.ready_fiat, c.price_age.map(prices::age)].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        ready_coins: c.ready_coins.into_iter().map(|c| CoinView { atomic: c.atomic, amount: c.amount, fiat: c.fiat, source: source(&c.source, &c.came) }).collect(),
        not_ready_title: format!("Not ready · {}", split::coins(c.not_ready_coins.len() as u32)),
        not_ready_sum: c.all_ready.filter(|_| !c.not_ready_coins.is_empty()).map_or(String::new(), |when| format!("all by {when}")),
        not_ready_coins: c.not_ready_coins.into_iter().map(piece).collect(),
        can_split: c.can_split,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_monero::fiat;
    use fob_screens::{clock::Clock, not_ready::{Piece, Source}};

    fn summarise(pieces: Vec<Piece>, ready: u64, last_block_time: u64, clock: Clock) -> Option<NotReady> {
        not_ready::summarise(pieces, ready, last_block_time, clock).map(self::not_ready)
    }

    fn coins(ready: Vec<Piece>, waiting: Vec<Piece>, last_block_time: u64, unlock: u64, price: Option<(&fiat::Currency, u64)>, age: Option<fiat::PriceAge>, clock: Clock) -> Coins {
        super::coins(not_ready::coins(ready, waiting, last_block_time, unlock, price, age, clock))
    }

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
            Some((&fiat::currency("USD"), 166_060_000)),
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
        let aged = self::coins(vec![piece(XMR, 10, Source::Received, NOW)], vec![], NOW, 10, Some((&fiat::currency("USD"), 166_060_000)), Some(fiat::PriceAge::Hours(23)), clock());
        assert_eq!(aged.ready_sum, "1 · $166.06 · 23 h old");
        let all_ready = self::coins(vec![piece(XMR, 10, Source::Received, NOW)], vec![], NOW, 10, None, None, clock());
        assert_eq!(all_ready.bar_description, "1 XMR ready");
    }

    #[test]
    fn each_sum_reads_as_one_line() {
        let n = summarise(vec![piece(XMR / 2, 5, Source::Received, NOW)], 2 * XMR, NOW, clock()).unwrap();
        let read: Vec<String> = n.sums.iter().map(|s| s.description.clone()).collect();
        assert_eq!(read, ["Ready to spend, 2 XMR", "Not ready · 1 coin, 0.5 XMR", "All of it, 2.5 XMR"]);
        let priced = sum(not_ready::Sum { kind: SumKind::Ready, atomic: 2 * XMR, amount: "2".into(), fiat: Some("$308.70".into()) });
        assert_eq!(priced.description, "Ready to spend, 2 XMR, $308.70");
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
        assert_eq!((n.ready, n.not_ready), (18_592 * XMR / 10_000, 1_253 * XMR / 1000));
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
