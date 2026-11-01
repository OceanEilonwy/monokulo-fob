//! Splitting up: paying what's ready to spend back to this wallet as
//! several pieces, so after the next payment there's still money ready.
//! A payment locks its change for 10 blocks (about 20 minutes); with the
//! money in pieces, only the piece spent waits.
//!
//! Pieces are notes: in the currency prices are shown in (six round amounts
//! from the currency's smallest note, 5, 10, 20, 50, 100, 200 in dollars,
//! 1,000 to 50,000 in yen, set in XMR at the price shown) or, without a
//! price, in XMR. This
//! module chooses them, checks them against the money and the 16 outputs a
//! transaction may have, and writes every word of the Split up screen.

use crate::{
    amount,
    price::{self, Currency},
};

/// The most pieces one split makes: a transaction has at most 16 outputs,
/// and one is the change.
pub const MAX_PIECES: u32 = 15;
/// How many notes there are to choose from, in money or XMR.
const NOTE_COUNT: usize = 6;
/// The notes in XMR, without a price: 0.01, 0.05, 0.1, 0.25, 0.5, 1.
const XMR_NOTES: [u64; NOTE_COUNT] = [10_000_000_000, 50_000_000_000, 100_000_000_000, 250_000_000_000, 500_000_000_000, 1_000_000_000_000];
/// How far back payments teach Auto: 30 days.
pub const LOOKBACK: u64 = 30 * 86_400;
/// Fewer payments than this, and there's nothing to learn from: no Auto.
const ENOUGH_PAYMENTS: usize = 3;
/// The fee per unit of weight before the node has said: about Monero's
/// usual normal fee.
const USUAL_PER_WEIGHT: u64 = 20_000;

pub const TITLE: &str = "Split up";
pub const INTRO: &str = "Each payment locks what's left of a coin for about 20 minutes. More, smaller coins keep money ready to spend.";
const AUTO: &str = "Auto";
const AUTO_SPOKEN: &str = "Auto, fill coins like your last 30 days";

/// The price pieces are set at: the currency and a price per XMR in
/// millionths, as elsewhere in [`price`].
pub type Money<'a> = Option<(&'a Currency, u64)>;

/// One kind of piece.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// What it is in XMR, in atomic units.
    pub atomic: u64,
    /// On its tile: "$20", "0.25".
    pub label: String,
    /// Read aloud and in lists: "$20", "0.25 XMR".
    pub spoken: String,
    /// In the currency's smallest unit, for a note in money.
    pub minor: Option<u64>,
}

/// A whole number of a currency: "$20", "20 CHF", "₫100,000".
fn whole(units: u64, currency: &Currency) -> String {
    let units = price::grouped(units);
    match currency.symbol {
        Some(symbol) => format!("{symbol}{units}"),
        None => format!("{units} {}", currency.code),
    }
}

/// The notes to choose from: in money at `money`'s price, or in XMR
/// without one.
pub fn notes(money: Money) -> Vec<Note> {
    if let Some((currency, p)) = money {
        let unit = 10u64.pow(currency.decimals);
        // Six round amounts from the currency's smallest note.
        let in_money: Option<Vec<Note>> = price::currencies::ladder(currency.note, NOTE_COUNT)
            .into_iter()
            .map(|units| {
                let minor = units.checked_mul(unit)?;
                let atomic = price::to_atomic(minor, p, currency).filter(|a| *a > 0)?;
                Some(Note { atomic, label: whole(units, currency), spoken: whole(units, currency), minor: Some(minor) })
            })
            .collect();
        if let Some(notes) = in_money.filter(|n| n.len() == NOTE_COUNT) {
            return notes;
        }
    }
    XMR_NOTES
        .iter()
        .map(|&atomic| Note { atomic, label: amount::format(atomic), spoken: format!("{} XMR", amount::format(atomic)), minor: None })
        .collect()
}

/// About what a transaction with `inputs` and `outputs` weighs, for a fee
/// shown before it's built: CLSAG inputs with 16-member rings, outputs to
/// a subaddress (an additional key each past two), and a Bulletproof+
/// over the outputs padded to a power of two, with Monero's clawback.
pub fn weight(inputs: usize, outputs: usize) -> u64 {
    let (inputs, outputs) = (inputs.max(1) as u64, outputs.max(2) as u64);
    let padded = outputs.next_power_of_two();
    // 32 bytes × (6 + 2 × log2(64 × padded)).
    let proof = 32 * (6 + 2 * (6 + u64::from(padded.trailing_zeros())));
    let clawback = if outputs > 2 { (320 * padded).saturating_sub(proof) * 4 / 5 } else { 0 };
    let additional_keys = if outputs > 2 { 32 * outputs } else { 0 };
    120 + 700 * inputs + 75 * outputs + additional_keys + proof + clawback
}

/// The money a split can use: the outputs ready to spend, and the fee rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    /// Each output ready to spend, in atomic units.
    pub outputs: Vec<u64>,
    /// The node's fee per unit of weight and the mask it rounds to, when
    /// known; Monero's usual rate otherwise.
    pub fee_rate: Option<(u64, u64)>,
}

impl Budget {
    pub fn spendable(&self) -> u64 {
        self.outputs.iter().sum()
    }

    fn fee_for(&self, inputs: usize, pieces: u32) -> u64 {
        let (per_weight, mask) = self.fee_rate.unwrap_or((USUAL_PER_WEIGHT, 10_000));
        let fee = per_weight.saturating_mul(weight(inputs, pieces as usize + 1));
        fee.div_ceil(mask.max(1)).saturating_mul(mask.max(1))
    }

    /// About the fee for `pieces` worth `value`, and what's left over as
    /// change; no change when it doesn't fit. Outputs are spent largest
    /// first, as [`Wallet::prepare`](crate::Wallet::prepare) does.
    pub fn plan(&self, pieces: u32, value: u64) -> (u64, Option<u64>) {
        let mut sorted = self.outputs.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let mut total = 0u64;
        for (i, output) in sorted.iter().enumerate() {
            total += output;
            let fee = self.fee_for(i + 1, pieces);
            if total >= value + fee {
                return (fee, Some(total - value - fee + sorted[i + 1..].iter().sum::<u64>()));
            }
        }
        (self.fee_for(sorted.len(), pieces), None)
    }

    fn fits(&self, pieces: u32, value: u64) -> bool {
        self.plan(pieces, value).1.is_some()
    }
}

fn pieces(counts: &[u32]) -> u32 {
    counts.iter().sum()
}

fn value(notes: &[Note], counts: &[u32]) -> u64 {
    notes.iter().zip(counts).map(|(n, c)| n.atomic * u64::from(*c)).sum()
}

/// `counts` as long as `notes`, any extra dropped.
fn sized(notes: &[Note], counts: &[u32]) -> Vec<u32> {
    (0..notes.len()).map(|i| counts.get(i).copied().unwrap_or(0)).collect()
}

/// Why `counts` can't be split, if they can't: too many pieces, or more
/// than is ready to spend with the fee.
pub fn check(notes: &[Note], counts: &[u32], budget: &Budget) -> Result<(), String> {
    let counts = sized(notes, counts);
    if pieces(&counts) > MAX_PIECES {
        return Err(format!("{MAX_PIECES} coins is the most one split can make"));
    }
    if !budget.fits(pieces(&counts), value(notes, &counts)) {
        return Err("That's more than is ready to spend, with the fee".into());
    }
    Ok(())
}

/// One more of note `i`, or why not.
pub fn add(notes: &[Note], counts: &[u32], i: usize, budget: &Budget) -> Result<Vec<u32>, String> {
    let mut counts = sized(notes, counts);
    let note = notes.get(i).ok_or("There's no such coin")?;
    if pieces(&counts) >= MAX_PIECES {
        return Err(format!("{MAX_PIECES} coins is the most one split can make"));
    }
    counts[i] += 1;
    if !budget.fits(pieces(&counts), value(notes, &counts)) {
        return Err(format!("Not enough ready to spend for another {} coin", note.spoken));
    }
    Ok(counts)
}

/// One fewer of note `i`.
pub fn remove(notes: &[Note], counts: &[u32], i: usize) -> Vec<u32> {
    let mut counts = sized(notes, counts);
    if let Some(c) = counts.get_mut(i) {
        *c = c.saturating_sub(1);
    }
    counts
}

/// Adds pieces one at a time where they're furthest below `weights`
/// (smaller notes first on a tie), skipping a note once another of it no
/// longer fits, until `limit` pieces or nothing fits.
fn fill(notes: &[Note], weights: &[u32], limit: u32, budget: &Budget) -> Vec<u32> {
    let mut counts = vec![0u32; notes.len()];
    let mut full = vec![false; notes.len()];
    while pieces(&counts) < limit.min(MAX_PIECES) {
        // The note whose next piece takes it least past its share:
        // (count + 1) ÷ weight, compared by cross-multiplying.
        let next = (0..notes.len())
            .filter(|&i| weights[i] > 0 && !full[i])
            .min_by(|&a, &b| (u64::from(counts[a] + 1) * u64::from(weights[b])).cmp(&(u64::from(counts[b] + 1) * u64::from(weights[a]))).then(a.cmp(&b)));
        let Some(i) = next else { break };
        counts[i] += 1;
        if !budget.fits(pieces(&counts), value(notes, &counts)) {
            counts[i] -= 1;
            full[i] = true;
        }
    }
    counts
}

/// The mix Auto fills, learned from `payments` (amounts paid in the
/// last 30 days, in atomic units): for each, the smallest note that covers
/// it (the largest for one bigger than all), and pieces added in those
/// proportions up to 15 or what's ready to spend. The rest stays as one
/// piece, the change. `None` with fewer than [`ENOUGH_PAYMENTS`], or when
/// not one piece fits: then there's no Auto.
pub fn learned(notes: &[Note], payments: &[u64], budget: &Budget) -> Option<Vec<u32>> {
    if payments.len() < ENOUGH_PAYMENTS || notes.is_empty() {
        return None;
    }
    let mut weights = vec![0u32; notes.len()];
    for &paid in payments {
        let i = notes.iter().position(|n| n.atomic >= paid).unwrap_or(notes.len() - 1);
        weights[i] += 1;
    }
    Some(fill(notes, &weights, MAX_PIECES, budget)).filter(|counts| pieces(counts) > 0)
}

/// Split up's Auto button, when there's a mix to learn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auto {
    /// What tapping it fills, in the order of the tiles.
    pub counts: Vec<u32>,
    /// "Auto"
    pub label: String,
    /// "Auto, fill coins like your last 30 days"
    pub spoken: String,
}

/// A tile on the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    /// "$20", "0.25"
    pub label: String,
    pub count: u32,
    /// "× 4", or "tap to add".
    pub count_text: String,
    /// "Add a $20 coin, 4 so far"
    pub add_label: String,
    /// "Remove a $20 coin"
    pub remove_label: String,
}

/// A block of the bar: one piece, or what's left over.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Its share of the bar's width, 0 to 1.
    pub width: f32,
    /// Its height as a share of the tallest, by the square root of its value.
    pub height: f32,
    /// What's left as one piece, drawn lighter.
    pub left_over: bool,
}

/// The Split up screen, every word ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub title: String,
    pub intro: String,
    /// Auto, with 3 or more payments in the last 30 days to learn from.
    pub auto: Option<Auto>,
    /// The counts are Auto's mix: the button is lit.
    pub is_auto: bool,
    pub tiles: Vec<Tile>,
    pub counts: Vec<u32>,
    pub pieces: u32,
    /// Pieces largest first, then what's left over.
    pub bar: Vec<Block>,
    /// "11 coins · $220"; `None` with no pieces.
    pub summary: Option<String>,
    /// "+ $88.70 left as one coin"
    pub left_over: Option<String>,
    /// "Split into 11 coins", or "Add a coin to split".
    pub button: String,
    pub ready: bool,
    /// "Network fee about $0.03", and the price's age once it's old:
    /// "Network fee about $0.03 · 23 h old price".
    pub footer: String,
}

fn worth(atomic: u64, money: Money) -> String {
    match money {
        Some((currency, p)) => price::value(atomic, p, currency),
        None => format!("{} XMR", amount::format_short(atomic)),
    }
}

/// The fee as the footer says it: "$0.03", "less than $0.01", "0.0002 XMR".
fn fee_text(fee: u64, money: Money) -> String {
    match money {
        Some((currency, p)) if price::to_minor(fee, p, currency) == 0 => format!("less than {}", price::format(1, currency)),
        Some((currency, p)) => price::value(fee, p, currency),
        None => format!("{} XMR", amount::format(fee)),
    }
}

fn plural(n: u32) -> String {
    if n == 1 { "1 coin".into() } else { format!("{n} coins") }
}

/// The screen for `counts` of `notes`, with Auto learned from `payments`
/// (see [`learned`]). `price_age` is how old the price is ("23 h old") once
/// it's missed a refresh, `None` while it's fresh.
pub fn view(notes: &[Note], counts: &[u32], payments: &[u64], budget: &Budget, money: Money, price_age: Option<&str>) -> View {
    let counts = sized(notes, counts);
    let auto = learned(notes, payments, budget).map(|counts| Auto { counts, label: AUTO.into(), spoken: AUTO_SPOKEN.into() });
    let n = pieces(&counts);
    let total = value(notes, &counts);
    let (fee, left) = budget.plan(n.max(1), total);
    let in_money = money.is_some() && notes.iter().all(|note| note.minor.is_some());
    let money = money.filter(|_| in_money);
    let tiles = notes
        .iter()
        .zip(&counts)
        .map(|(note, &count)| Tile {
            label: note.label.clone(),
            count,
            count_text: if count > 0 { format!("× {count}") } else { "tap to add".into() },
            add_label: format!("Add a {} coin, {} so far", note.spoken, if count == 0 { "none".to_owned() } else { count.to_string() }),
            remove_label: format!("Remove a {} coin", note.spoken),
        })
        .collect();
    // The pieces largest first, then what's left over.
    let mut values: Vec<(u64, bool)> = vec![];
    for (note, &count) in notes.iter().zip(&counts).rev() {
        values.extend(std::iter::repeat_n((note.atomic, false), count as usize));
    }
    if let Some(left) = left.filter(|l| *l > 0 && n > 0) {
        values.push((left, true));
    }
    let whole: u64 = values.iter().map(|v| v.0).sum();
    let tallest = values.iter().map(|v| (v.0 as f64).sqrt()).fold(0.0, f64::max);
    let bar = values
        .iter()
        .map(|&(v, left_over)| Block { width: (v as f64 / whole as f64) as f32, height: ((v as f64).sqrt() / tallest) as f32, left_over })
        .collect();
    let summary_value = match money {
        Some((currency, _)) => price::format(notes.iter().zip(&counts).map(|(note, c)| note.minor.unwrap_or(0) * u64::from(*c)).sum(), currency),
        None => format!("{} XMR", amount::format_short(total)),
    };
    let mut footer = format!("Network fee about {}", fee_text(fee, money));
    if let Some(age) = price_age.filter(|_| money.is_some()) {
        footer.push_str(&format!(" · {age} price"));
    }
    View {
        title: TITLE.into(),
        intro: INTRO.into(),
        is_auto: auto.as_ref().is_some_and(|a| a.counts == counts),
        auto,
        tiles,
        pieces: n,
        bar,
        summary: (n > 0).then(|| format!("{} · {summary_value}", plural(n))),
        left_over: left.filter(|l| *l > 0 && n > 0).map(|l| format!("+ {} left as one coin", worth(l, money))),
        button: if n == 0 { "Add a coin to split".into() } else { format!("Split into {}", plural(n)) },
        ready: n > 0 && left.is_some() && n <= MAX_PIECES,
        footer,
        counts,
    }
}

/// The pieces of `counts`, largest first, in atomic units: the outputs a
/// split pays.
pub fn amounts(notes: &[Note], counts: &[u32]) -> Vec<u64> {
    let counts = sized(notes, counts);
    notes.iter().zip(&counts).rev().flat_map(|(note, &c)| std::iter::repeat_n(note.atomic, c as usize)).collect()
}

/// The confirm screen's list: "$20 × 4" with "0.2408 XMR", largest first.
pub fn lines(notes: &[Note], counts: &[u32]) -> Vec<(String, String)> {
    let counts = sized(notes, counts);
    notes
        .iter()
        .zip(&counts)
        .rev()
        .filter(|(_, c)| **c > 0)
        .map(|(note, &c)| (format!("{} × {c}", note.spoken), format!("{} XMR", amount::format(note.atomic * u64::from(c)))))
        .collect()
}

/// "12 coins", for Activity and the confirm screen.
pub fn pieces_text(n: u32) -> String {
    plural(n)
}

/// Where a split pays, on the confirm screen.
pub const TO: &str = "This wallet";

/// The confirm screen's swipe: "Swipe to split into 12 coins".
pub fn swipe_text(n: u32) -> String {
    format!("Swipe to split into {}", plural(n))
}

/// Under "Split up" once sent: "12 coins on their way back to this wallet".
pub fn done_text(n: u32) -> String {
    format!("{} on their way back to this wallet", plural(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    const XMR: u64 = amount::ATOMIC_PER_XMR;

    fn usd() -> Currency {
        price::currency("USD")
    }

    /// $300.00 per XMR.
    const PRICE: u64 = 300 * price::MICRO;

    fn budget(xmr_thousandths: u64) -> Budget {
        Budget { outputs: vec![xmr_thousandths * XMR / 1000], fee_rate: Some((20_000, 10_000)) }
    }

    #[test]
    fn notes_are_money_at_todays_price_or_xmr() {
        let usd = usd();
        let notes = notes(Some((&usd, PRICE)));
        let labels: Vec<_> = notes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["$5", "$10", "$20", "$50", "$100", "$200"]);
        // $20 at $300 is 0.06667 XMR: as few decimals as come to $20.00
        // (0.0667 would be $20.01).
        assert_eq!(amount::format(notes[2].atomic), "0.06667");
        assert_eq!(price::to_minor(notes[2].atomic, PRICE, &usd), 2_000);
        let (gbp, chf) = (price::currency("GBP"), price::currency("CHF"));
        assert_eq!(super::notes(Some((&gbp, PRICE)))[0].label, "£5");
        assert_eq!(super::notes(Some((&chf, PRICE)))[0].label, "5 CHF");
        // Currencies with many units to the dollar get notes of about the
        // same worth, not 5 yen.
        let labels = |code: &str, price: u64| super::notes(Some((&price::currency(code), price))).into_iter().map(|n| n.label).collect::<Vec<_>>();
        assert_eq!(labels("JPY", 45_000 * price::MICRO), ["¥1,000", "¥2,000", "¥5,000", "¥10,000", "¥20,000", "¥50,000"]);
        assert_eq!(labels("VND", 7_500_000 * price::MICRO), ["₫100,000", "₫200,000", "₫500,000", "₫1,000,000", "₫2,000,000", "₫5,000,000"]);
        assert_eq!(labels("KRW", 420_000 * price::MICRO)[0], "₩5,000");
        assert_eq!(labels("IDR", 4_800_000 * price::MICRO)[0], "Rp50,000");
        assert_eq!(labels("KWD", 92 * price::MICRO), ["1 KWD", "2 KWD", "5 KWD", "10 KWD", "20 KWD", "50 KWD"]);
        // ¥1,000 at ¥45,000 an XMR is about the same XMR as $5 at $300.
        let yen = super::notes(Some((&price::currency("JPY"), 45_000 * price::MICRO)));
        assert_eq!(amount::format(yen[0].atomic), "0.02222");
        assert_eq!(amount::format(notes[0].atomic), "0.01667");
        let xmr: Vec<_> = super::notes(None).iter().map(|n| (n.label.clone(), n.spoken.clone())).collect();
        assert_eq!(xmr[0], ("0.01".into(), "0.01 XMR".into()));
        assert_eq!(xmr.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["0.01", "0.05", "0.1", "0.25", "0.5", "1"]);
        // A price of 0 can't set notes in money.
        assert_eq!(super::notes(Some((&usd, 0)))[0].label, "0.01");
    }

    #[test]
    fn weight_grows_with_outputs_and_inputs() {
        assert_eq!(weight(1, 2), 120 + 700 + 150 + 640);
        assert!(weight(1, 16) > weight(1, 9));
        assert!(weight(2, 16) - weight(1, 16) == 700);
        // 15 pieces and change, from two inputs, at the usual rate: about 0.0002 XMR.
        let fee = budget(1000).fee_for(2, 15);
        assert!((100_000_000..300_000_000).contains(&fee), "{fee}");
    }

    #[test]
    fn the_budget_spends_largest_first_and_leaves_the_rest() {
        let b = Budget { outputs: vec![XMR / 10, XMR, XMR / 2], fee_rate: Some((20_000, 10_000)) };
        let (fee, left) = b.plan(3, XMR);
        // One input can't cover 1 XMR and the fee; two can.
        assert_eq!(fee, b.fee_for(2, 3));
        assert_eq!(left, Some(XMR / 10 + XMR / 2 - fee));
        assert_eq!(b.plan(3, 16 * XMR / 10).1, None);
        assert_eq!(b.spendable(), 16 * XMR / 10);
    }

    #[test]
    fn adding_stops_at_fifteen_pieces_or_the_money() {
        let notes = notes(None);
        let plenty = budget(100_000);
        let mut counts = vec![];
        for _ in 0..MAX_PIECES {
            counts = add(&notes, &counts, 0, &plenty).unwrap();
        }
        assert_eq!(counts, [15, 0, 0, 0, 0, 0]);
        assert_eq!(add(&notes, &counts, 1, &plenty).unwrap_err(), "15 coins is the most one split can make");
        // 0.6 XMR: one 0.5 and then no 0.25 with the fee.
        let little = budget(600);
        let counts = add(&notes, &[], 4, &little).unwrap();
        assert_eq!(add(&notes, &counts, 3, &little).unwrap_err(), "Not enough ready to spend for another 0.25 XMR coin");
        assert!(add(&notes, &counts, 1, &little).is_ok());
        assert_eq!(remove(&notes, &counts, 4), [0; 6]);
        assert_eq!(remove(&notes, &[], 4), [0; 6]);
        assert!(check(&notes, &[16], &plenty).is_err());
        assert!(check(&notes, &[0, 0, 0, 0, 0, 1], &little).is_err());
        assert!(check(&notes, &[0, 0, 0, 0, 1], &little).is_ok());
    }

    #[test]
    fn auto_learns_from_payments() {
        let usd = usd();
        let money = Some((&usd, PRICE));
        let notes = notes(money);
        let dollars = |d: u64| price::to_atomic(d * 100, PRICE, &usd).unwrap();
        // Coffees and lunches: $4, $5, $6, $12, $18, $25, $25.
        let payments = [dollars(4), dollars(5), dollars(6), dollars(12), dollars(18), dollars(25), dollars(25)];
        // 2 XMR is $600: plenty for 15 pieces.
        let mix = learned(&notes, &payments, &budget(2_000)).unwrap();
        // Shares 2 × $5, 1 × $10, 2 × $20, 2 × $50 of 7, filled to 15 in turn.
        assert_eq!(mix, [5, 2, 4, 4, 0, 0]);
        assert_eq!(view(&notes, &mix, &payments, &budget(2_000), money, None).summary.as_deref(), Some("15 coins · $325.00"));
        // The same payments with $90: pieces in turn while they fit, $85.
        let small = learned(&notes, &payments, &budget(300)).unwrap();
        assert_eq!(small, [3, 0, 1, 1, 0, 0]);
        assert!(check(&notes, &small, &budget(300)).is_ok());
        assert!(value(&notes, &small) <= budget(300).spendable());
        // Deterministic.
        assert_eq!(learned(&notes, &payments, &budget(300)).unwrap(), small);
    }

    #[test]
    fn auto_stays_within_fifteen_coins_and_the_money() {
        let notes = notes(None);
        // Payments that agree: all of one note, up to 15.
        assert_eq!(learned(&notes, &[XMR / 10; 4], &budget(5_000)).unwrap(), [0, 0, 15, 0, 0, 0]);
        // A payment bigger than every note counts as the largest; 5 XMR
        // holds 4 of them with the fee.
        assert_eq!(learned(&notes, &[3 * XMR; 3], &budget(5_000)).unwrap(), [0, 0, 0, 0, 0, 4]);
        // Whatever the money, the mix is a split that can be made.
        for thousandths in [15, 100, 600, 1_000, 2_000, 5_000, 100_000] {
            let b = budget(thousandths);
            let payments = [XMR / 100, XMR / 20, XMR / 4, XMR, 2 * XMR];
            if let Some(mix) = learned(&notes, &payments, &b) {
                assert!(pieces(&mix) <= MAX_PIECES && pieces(&mix) > 0, "{thousandths}: {mix:?}");
                assert!(check(&notes, &mix, &b).is_ok(), "{thousandths}: {mix:?}");
            }
        }
        assert_eq!(pieces(&learned(&notes, &[XMR / 100; 30], &budget(100_000)).unwrap()), MAX_PIECES);
        // Nothing ready, or not even the smallest note with the fee: no Auto.
        assert_eq!(learned(&notes, &[XMR / 10; 4], &budget(0)), None);
        assert_eq!(learned(&notes, &[XMR / 10; 4], &budget(10)), None);
    }

    #[test]
    fn auto_is_offered_only_with_three_payments_or_more() {
        let notes = notes(None);
        let b = budget(5_000);
        for few in [&[][..], &[XMR / 10], &[XMR / 10, XMR / 5]] {
            assert_eq!(learned(&notes, few, &b), None);
            let v = view(&notes, &[], few, &b, None, None);
            assert!(v.auto.is_none() && !v.is_auto);
        }
        let v = view(&notes, &[], &[XMR / 10; 3], &b, None, None);
        let auto = v.auto.unwrap();
        assert_eq!((auto.label.as_str(), auto.spoken.as_str()), ("Auto", "Auto, fill coins like your last 30 days"));
        assert_eq!(auto.counts, [0, 0, 15, 0, 0, 0]);
    }

    #[test]
    fn auto_is_lit_while_the_coins_are_its_mix() {
        let notes = notes(None);
        let b = budget(5_000);
        let payments = [XMR / 100, XMR / 10, XMR / 10, XMR / 4];
        let lit = |counts: &[u32]| view(&notes, counts, &payments, &b, None, None).is_auto;
        // The screen opens empty: no coins, Auto plain, nothing to split yet.
        let opening = view(&notes, &[], &payments, &b, None, None);
        assert_eq!((opening.counts.as_slice(), opening.pieces, opening.is_auto), (&[0; 6][..], 0, false));
        assert_eq!((opening.button.as_str(), opening.ready), ("Add a coin to split", false));
        // Tapping Auto fills its mix: lit.
        let mix = opening.auto.unwrap().counts;
        assert!(lit(&mix));
        // The same coins with the zeros at the end left off: still lit.
        let trimmed: Vec<u32> = mix.iter().copied().take(mix.iter().rposition(|c| *c > 0).unwrap() + 1).collect();
        assert!(trimmed.len() < mix.len() && lit(&trimmed));
        // Change a coin and it goes plain; put it back and it's lit again.
        let fewer = remove(&notes, &mix, 2);
        assert!(!lit(&fewer));
        let back = add(&notes, &fewer, 2, &b).unwrap();
        assert_eq!(back, mix);
        assert!(lit(&back));
        assert!(!lit(&add(&notes, &fewer, 0, &b).unwrap()));
        // Clear: plain.
        assert!(!lit(&[]));
    }

    #[test]
    fn the_screen_reads_like_the_design() {
        let usd = usd();
        let money = Some((&usd, PRICE));
        let notes = notes(money);
        // $5 × 4, $10 × 4, $20 × 3: 11 pieces, $120. With 1 XMR ($300) ready.
        let b = budget(1_000);
        let v = view(&notes, &[4, 4, 3], &[], &b, money, None);
        assert_eq!(v.summary.as_deref(), Some("11 coins · $120.00"));
        let (fee, left) = b.plan(11, value(&notes, &[4, 4, 3]));
        assert_eq!(v.left_over, Some(format!("+ {} left as one coin", price::value(left.unwrap(), PRICE, &usd))));
        assert_eq!(v.button, "Split into 11 coins");
        assert!(v.ready);
        assert_eq!(v.footer, format!("Network fee about {}", price::value(fee, PRICE, &usd)));
        // An old price: its age after the fee, and nothing more.
        let aged = view(&notes, &[4, 4, 3], &[], &b, money, Some("23 h old"));
        assert_eq!(aged.footer, format!("Network fee about {} · 23 h old price", price::value(fee, PRICE, &usd)));
        assert_eq!(v.counts, [4, 4, 3, 0, 0, 0]);
        let tile = |i: usize| (v.tiles[i].label.as_str(), v.tiles[i].count_text.as_str(), v.tiles[i].add_label.as_str());
        assert_eq!(tile(2), ("$20", "× 3", "Add a $20 coin, 3 so far"));
        assert_eq!(tile(5), ("$200", "tap to add", "Add a $200 coin, none so far"));
        assert_eq!(v.tiles[0].remove_label, "Remove a $5 coin");
        // The bar: 11 pieces largest first, then what's left, lighter.
        assert_eq!(v.bar.len(), 12);
        assert!(v.bar[11].left_over && !v.bar[0].left_over);
        assert!((v.bar.iter().map(|b| b.width).sum::<f32>() - 1.0).abs() < 0.001);
        // The $180 left over is the tallest; a $20 is √(20 ÷ 180) of it.
        assert_eq!(v.bar[11].height, 1.0);
        assert!((v.bar[0].height - (20.0f32 / 180.0).sqrt()).abs() < 0.01, "{}", v.bar[0].height);

        let empty = view(&notes, &[], &[], &b, money, None);
        assert_eq!((empty.button.as_str(), empty.ready, empty.summary, empty.left_over, empty.bar.len()), ("Add a coin to split", false, None, None, 0));

        // In XMR there's no price, so no age even if one was given.
        let xmr = view(&super::notes(None), &[0, 0, 1], &[], &b, None, Some("23 h old"));
        assert_eq!(xmr.summary.as_deref(), Some("1 coin · 0.1 XMR"));
        assert!(xmr.footer.starts_with("Network fee about 0.000") && !xmr.footer.contains("old"), "{}", xmr.footer);
        assert_eq!(xmr.button, "Split into 1 coin");
    }

    #[test]
    fn the_pieces_paid_and_listed_largest_first() {
        let notes = notes(None);
        assert_eq!(amounts(&notes, &[2, 0, 1]), [XMR / 10, XMR / 100, XMR / 100]);
        assert_eq!(lines(&notes, &[2, 0, 1]), [("0.1 XMR × 1".to_owned(), "0.1 XMR".to_owned()), ("0.01 XMR × 2".to_owned(), "0.02 XMR".to_owned())]);
        assert_eq!(pieces_text(12), "12 coins");
        assert_eq!(swipe_text(1), "Swipe to split into 1 coin");
        assert_eq!(done_text(12), "12 coins on their way back to this wallet");
        assert_eq!(fee_text(1, Some((&usd(), PRICE))), "less than $0.01");
    }
}
