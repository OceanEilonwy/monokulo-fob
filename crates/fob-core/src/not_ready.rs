//! Money that isn't ready to spend yet, piece by piece: each output the
//! wallet owns that hasn't its 10 confirmations (a mined reward, its 60),
//! where it came from, and about when it will be ready. Home folds it into
//! one line beside what's ready, with a bar, and opens it into sums.

use crate::{
    activity::{Clock, local, time_of_day},
    amount,
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
    /// "Received · 08:40", "Change · Corner Café".
    pub source: String,
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
pub(crate) fn coins(n: usize) -> String {
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
                source: match p.source {
                    Source::Received => format!("Received · {}", clock_time(p.time, clock)),
                    Source::Change(Some(name)) => format!("Change · {name}"),
                    Source::Change(None) => "Change".into(),
                },
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
            sum(format!("Not ready · {}", coins(views.len())), total),
            sum("All of it".into(), ready + total),
        ],
        progress: progress as f32,
        pieces: views,
    })
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
