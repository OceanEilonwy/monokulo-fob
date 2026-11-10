//! The wallet's coins: each output it owns, ready to spend or not yet. Money
//! not ready is listed piece by piece: each output that hasn't its 10
//! confirmations (a mined reward, its 60), where it came from, and about
//! when it will be ready. Home folds it into one line beside what's ready,
//! with a bar, and opens it into sums; the Coins screen lists every coin.

use fob_monero::{amount, fiat};
use crate::clock::{Clock, local, short_when, time_of_day};

/// About how long a block takes.
const BLOCK_SECONDS: u64 = 120;

/// The most blocks a piece is counted as still waiting: about ten years.
/// Anyone can send a dust output time-locked to near u64::MAX, and waiting
/// blocks that far out overflow the seconds (and the clock's i64) instead of
/// saying anything useful, so "ready" stops at a date that already reads as
/// never.
const MAX_WAIT_BLOCKS: u64 = 10 * 365 * 24 * 3600 / BLOCK_SECONDS;

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
    pub source: Source,
    /// When it came: "08:40", "22 Oct".
    pub came: String,
    /// When it should be ready: "09:02".
    pub ready: String,
    /// How long until then.
    pub seconds_left: u64,
    /// Seconds since the epoch.
    pub ready_at: u64,
}

/// Which sum a line of Home's opened card is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SumKind {
    Ready,
    NotReady { coins: usize },
    All,
}

/// A line of Home's opened card.
#[derive(Clone, Debug, PartialEq)]
pub struct Sum {
    pub kind: SumKind,
    /// In atomic units.
    pub atomic: u64,
    /// "1.8592"
    pub amount: String,
    /// What it's worth now, when prices are on: "$308.70".
    pub fiat: Option<String>,
}

/// All the money not ready yet, beside what is.
#[derive(Clone, Debug, PartialEq)]
pub struct NotReady {
    /// Not ready, in atomic units.
    pub total: u64,
    /// Ready to spend, in atomic units.
    pub ready: u64,
    /// When the last of it should be ready: "09:02".
    pub all_ready: String,
    /// How long until then.
    pub seconds_left: u64,
    /// Ready to spend, not ready, and all of it.
    pub sums: Vec<Sum>,
    /// How old the price the sums are worth at is, once it's missed a refresh.
    pub price_age: Option<fiat::PriceAge>,
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

fn clock_time(time: u64, clock: Clock) -> String {
    time_of_day(local(time, clock).1, clock)
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
            let waiting = (p.needed - p.confirmations).min(MAX_WAIT_BLOCKS);
            let ready_at = from.saturating_add(waiting * BLOCK_SECONDS);
            PieceView {
                atomic: p.amount,
                amount: amount::format_short(p.amount),
                fiat: None,
                confirmations: p.confirmations,
                needed: p.needed,
                progress: p.confirmations as f32 / p.needed as f32,
                source: p.source,
                came: short_when(p.time, clock),
                ready: clock_time(ready_at, clock),
                seconds_left: ready_at.saturating_sub(clock.now),
                ready_at,
            }
        })
        .collect();
    views.sort_by_key(|v| v.ready_at);
    let last = views.last().expect("a piece");
    let (all_ready, seconds_left) = (last.ready.clone(), last.seconds_left);
    let sum = |kind: SumKind, atomic: u64| Sum { kind, atomic, amount: amount::format_short(atomic), fiat: None };
    Some(NotReady {
        total,
        ready,
        all_ready,
        seconds_left,
        sums: vec![sum(SumKind::Ready, ready), sum(SumKind::NotReady { coins: views.len() }, total), sum(SumKind::All, ready + total)],
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
    pub source: Source,
    /// When it came: "08:40", "22 Oct".
    pub came: String,
}

/// What the Coins screen says the money is held as, once there's a coin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    /// "2.4813"
    pub amount: String,
    pub coins: usize,
    /// About how long what's left of a coin isn't ready.
    pub minutes: u64,
}

/// The Coins screen: every coin, ready or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Coins {
    /// `None` with no coins yet.
    pub held: Option<Held>,
    /// Ready to spend and not ready, in atomic units: the bar's two parts.
    pub ready: u64,
    pub not_ready: u64,
    /// The not-ready part's weighted progress, as on Home.
    pub progress: f32,
    /// What's ready: "1.8592".
    pub ready_amount: String,
    /// What it's worth, with a price: "$308.70".
    pub ready_fiat: Option<String>,
    /// How old that price is, once it's missed a refresh.
    pub price_age: Option<fiat::PriceAge>,
    /// Largest first.
    pub ready_coins: Vec<CoinView>,
    /// When the last not ready should be ready: "09:02".
    pub all_ready: Option<String>,
    /// Soonest ready first.
    pub not_ready_coins: Vec<PieceView>,
    /// Split up can be pressed: some coin is ready to spend. Without one
    /// (no coins, or none ready yet) the button shows but is greyed out.
    pub can_split: bool,
}

/// Every coin as the Coins screen shows it: the `ready` ones and those not
/// ready yet, valued at `price` when there is one. A new coin, change
/// included, waits `unlock` blocks. `age` says how old the price is once it's
/// missed a refresh, beside the ready sum.
pub fn coins(ready: Vec<Piece>, not_ready: Vec<Piece>, last_block_time: u64, unlock: u64, price: Option<(&fiat::Currency, u64)>, age: Option<fiat::PriceAge>, clock: Clock) -> Coins {
    let worth = |atomic: u64| price.map(|(currency, p)| fiat::value(atomic, p, currency));
    let ready_total: u64 = ready.iter().map(|p| p.amount).sum();
    let mut ready_coins: Vec<CoinView> = ready
        .into_iter()
        .filter(|p| p.amount > 0)
        .map(|p| CoinView { atomic: p.amount, amount: amount::format_short(p.amount), fiat: worth(p.amount), came: short_when(p.time, clock), source: p.source })
        .collect();
    ready_coins.sort_by_key(|c| std::cmp::Reverse(c.atomic));
    let summary = summarise(not_ready, ready_total, last_block_time, clock);
    let (not_ready_total, progress, all_ready, mut pieces) = match summary {
        Some(n) => (n.total, n.progress, Some(n.all_ready), n.pieces),
        None => (0, 0.0, None, vec![]),
    };
    for piece in &mut pieces {
        piece.fiat = worth(piece.atomic);
    }
    let count = ready_coins.len() + pieces.len();
    Coins {
        held: (count > 0).then(|| Held { amount: amount::format_short(ready_total + not_ready_total), coins: count, minutes: unlock * BLOCK_SECONDS / 60 }),
        ready: ready_total,
        not_ready: not_ready_total,
        progress,
        ready_amount: amount::format_short(ready_total),
        ready_fiat: worth(ready_total),
        price_age: worth(ready_total).and(age),
        ready_coins,
        all_ready,
        not_ready_coins: pieces,
        can_split: ready_total > 0,
    }
}

/// A payment Confirm can't make: what it needs against what's ready, in
/// atomic units.
#[derive(Clone, Debug, PartialEq)]
pub struct Shortfall {
    /// The amount with about the fee.
    pub needed: u64,
    /// Ready to spend now.
    pub ready: u64,
    /// Not ready yet: received, or change, still settling.
    pub on_way: u64,
    /// `needed` less `ready`.
    pub short: u64,
    /// When money on its way covers it: about how long until enough is
    /// ready, and how far along the coin that tips it is, for the bar.
    /// `None` when even all of it wouldn't.
    pub ready_after: Option<(u64, f32)>,
}

/// What paying `needed` lacks, with `ready` spendable and `not_ready` (from
/// [`summarise`]) still settling, its pieces soonest ready first.
pub fn shortfall(needed: u64, ready: u64, not_ready: Option<&NotReady>) -> Shortfall {
    let on_way = not_ready.map_or(0, |n| n.total);
    let mut have = ready;
    let mut ready_after = None;
    for piece in not_ready.map(|n| n.pieces.as_slice()).unwrap_or_default() {
        if have >= needed {
            break;
        }
        have = have.saturating_add(piece.atomic);
        if have >= needed {
            ready_after = Some((piece.seconds_left, piece.progress));
        }
    }
    Shortfall { needed, ready, on_way, short: needed.saturating_sub(ready), ready_after }
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
    fn a_shortfall_says_when_money_on_its_way_covers_it() {
        // 0.3 ready; 0.2 with 7 of 10 confirmations (about 6 min), 0.5 with 2 (about 16 min).
        let pieces = vec![piece(XMR / 5, 7, Source::Received, NOW - 600), piece(XMR / 2, 2, Source::Received, NOW - 300)];
        let waiting = summarise(pieces, 3 * XMR / 10, NOW, clock());
        // 0.45 needs the first coin only.
        let s = shortfall(45 * XMR / 100, 3 * XMR / 10, waiting.as_ref());
        assert_eq!((s.short, s.on_way), (15 * XMR / 100, 7 * XMR / 10));
        assert_eq!(s.ready_after, Some((3 * 120, 0.7)));
        // 0.9 needs both.
        assert_eq!(shortfall(9 * XMR / 10, 3 * XMR / 10, waiting.as_ref()).ready_after, Some((8 * 120, 0.2)));
        // 2 XMR is more than everything: short, with nothing to wait for.
        let never = shortfall(2 * XMR, 3 * XMR / 10, waiting.as_ref());
        assert_eq!((never.short, never.ready_after), (17 * XMR / 10, None));
        // Nothing on its way at all.
        let none = shortfall(XMR, 3 * XMR / 10, None);
        assert_eq!((none.on_way, none.ready_after), (0, None));
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
    fn a_coin_says_where_it_came_from_and_how_long_without_words() {
        let n = summarise(vec![piece(XMR / 2, 5, Source::Received, NOW)], 2 * XMR, NOW, clock()).unwrap();
        assert_eq!(n.sums.iter().map(|s| s.kind).collect::<Vec<_>>(), [SumKind::Ready, SumKind::NotReady { coins: 1 }, SumKind::All]);
        assert_eq!((n.pieces[0].source.clone(), n.pieces[0].came.as_str(), n.seconds_left), (Source::Received, "08:48", 600));
        let empty = coins(vec![], vec![], NOW, 10, None, None, clock());
        assert_eq!(empty.held, None);
    }

    #[test]
    fn a_coin_locked_until_the_end_of_time_neither_panics_nor_wraps() {
        // A time lock near u64::MAX makes the wallet ask for about
        // u64::MAX / 120 confirmations; that used to overflow every Home draw.
        let locked = Piece { amount: 1, confirmations: 0, needed: u64::MAX / BLOCK_SECONDS + 5, source: Source::Received, time: NOW };
        let n = summarise(vec![locked.clone(), piece(XMR, 3, Source::Received, NOW)], 0, NOW, clock()).unwrap();
        let last = n.pieces.last().unwrap();
        assert_eq!(last.atomic, 1, "the locked coin is the last ready");
        assert_eq!(last.ready_at, NOW + MAX_WAIT_BLOCKS * BLOCK_SECONDS);
        assert!(last.seconds_left > 9 * 365 * 24 * 3600, "not ready any time soon");
        assert_eq!(n.seconds_left, last.seconds_left);
        let all = coins(vec![], vec![Piece { needed: u64::MAX, ..locked }], NOW, 10, None, None, clock());
        assert_eq!(all.not_ready_coins[0].ready_at, NOW + MAX_WAIT_BLOCKS * BLOCK_SECONDS);
    }
}
