//! Split up, in words.

use fob_screens::split::{self, FeeAmount, SplitFee};

use crate::prices;

pub const TITLE: &str = "Split up";
pub const INTRO: &str = "Each payment locks what's left of a coin for about 20 minutes. More, smaller coins keep money ready to spend.";
const AUTO: &str = "Auto";
const AUTO_SPOKEN: &str = "Auto, fill coins like your last 30 days";

/// Where a split pays, on the confirm screen.
pub const TO: &str = "This wallet";

/// "1 coin", "12 coins".
pub fn coins(n: u32) -> String {
    if n == 1 { "1 coin".into() } else { format!("{n} coins") }
}

/// The confirm screen's swipe: "Swipe to split into 12 coins".
pub fn swipe_text(n: u32) -> String {
    format!("Swipe to split into {}", coins(n))
}

/// Under "Split up" once sent: "12 coins on their way back to this wallet".
pub fn done_text(n: u32) -> String {
    format!("{} on their way back to this wallet", coins(n))
}

/// Split up's Auto button, as it's said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auto {
    /// What tapping it fills, in the order of the tiles.
    pub counts: Vec<u32>,
    /// "Auto"
    pub label: String,
    /// "Auto, fill coins like your last 30 days"
    pub spoken: String,
}

/// A tile on the grid, as it's said.
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

/// The Split up screen as it's said.
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
    pub bar: Vec<split::Block>,
    /// "11 coins · $220"; `None` with no pieces.
    pub summary: Option<String>,
    /// "+ $88.70 left as one coin"
    pub left_over: Option<String>,
    /// "Split into 11 coins", or "Add a coin to split".
    pub button: String,
    pub ready: bool,
    /// "Network fee about $0.03", and the price's age once it's old:
    /// "Network fee about $0.03 · 23 h old price"; empty while no coin is
    /// chosen.
    pub footer: String,
}

fn fee(fee: &SplitFee) -> String {
    let amount = match &fee.fee {
        FeeAmount::About(text) => text.clone(),
        FeeAmount::LessThan(text) => format!("less than {text}"),
    };
    match fee.age {
        Some(age) => format!("Network fee about {amount} · {} price", prices::age(age)),
        None => format!("Network fee about {amount}"),
    }
}

pub fn view(v: split::View) -> View {
    let tile = |t: split::Tile| Tile {
        count_text: if t.count > 0 { format!("× {}", t.count) } else { "tap to add".into() },
        add_label: format!("Add a {} coin, {} so far", t.spoken, if t.count == 0 { "none".to_owned() } else { t.count.to_string() }),
        remove_label: format!("Remove a {} coin", t.spoken),
        label: t.label,
        count: t.count,
    };
    View {
        title: TITLE.into(),
        intro: INTRO.into(),
        auto: v.auto.map(|a| Auto { counts: a.counts, label: AUTO.into(), spoken: AUTO_SPOKEN.into() }),
        is_auto: v.is_auto,
        tiles: v.tiles.into_iter().map(tile).collect(),
        counts: v.counts,
        pieces: v.pieces,
        bar: v.bar,
        summary: v.total.map(|total| format!("{} · {total}", coins(v.pieces))),
        left_over: v.left_over.map(|left| format!("+ {left} left as one coin")),
        button: if v.pieces == 0 { "Add a coin to split".into() } else { format!("Split into {}", coins(v.pieces)) },
        ready: v.ready,
        footer: v.fee.as_ref().map_or(String::new(), fee),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_monero::{amount, fiat::{self, Currency, PriceAge}};
    use fob_screens::split::{Budget, Money, Note, notes};

    fn view(notes: &[Note], counts: &[u32], payments: &[u64], budget: &Budget, money: Money, age: Option<PriceAge>) -> View {
        super::view(split::view(notes, counts, payments, budget, money, age))
    }

    const XMR: u64 = amount::ATOMIC_PER_XMR;

    fn usd() -> Currency {
        fiat::currency("USD")
    }

    /// $300.00 per XMR.
    const PRICE: u64 = 300 * fiat::MICRO;

    fn budget(xmr_thousandths: u64) -> Budget {
        Budget { outputs: vec![xmr_thousandths * XMR / 1000], fee_rate: Some((20_000, 10_000)) }
    }

    #[test]
    fn the_screen_reads_like_the_design() {
        let usd = usd();
        let money = Some((&usd, PRICE));
        let notes = notes(money);
        let value = |counts: &[u32]| notes.iter().zip(counts).map(|(n, c)| n.atomic * u64::from(*c)).sum::<u64>();
        // $5 × 4, $10 × 4, $20 × 3: 11 pieces, $120. With 1 XMR ($300) ready.
        let b = budget(1_000);
        let v = view(&notes, &[4, 4, 3], &[], &b, money, None);
        assert_eq!(v.summary.as_deref(), Some("11 coins · $120.00"));
        let (fee, left) = b.plan(11, value(&[4, 4, 3]));
        assert_eq!(v.left_over, Some(format!("+ {} left as one coin", fiat::value(left.unwrap(), PRICE, &usd))));
        assert_eq!(v.button, "Split into 11 coins");
        assert!(v.ready);
        assert_eq!(v.footer, format!("Network fee about {}", fiat::value(fee, PRICE, &usd)));
        // An old price: its age after the fee, and nothing more.
        let aged = view(&notes, &[4, 4, 3], &[], &b, money, Some(PriceAge::Hours(23)));
        assert_eq!(aged.footer, format!("Network fee about {} · 23 h old price", fiat::value(fee, PRICE, &usd)));
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

        let empty = view(&notes, &[], &[], &b, money, Some(PriceAge::Hours(23)));
        assert_eq!((empty.button.as_str(), empty.ready, empty.summary, empty.left_over, empty.bar.len()), ("Add a coin to split", false, None, None, 0));
        assert_eq!(empty.footer, "", "no fee for a split nobody has asked for");

        // In XMR there's no price, so no age even if one was given.
        let xmr = view(&fob_screens::split::notes(None), &[0, 0, 1], &[], &b, None, Some(PriceAge::Hours(23)));
        assert_eq!(xmr.summary.as_deref(), Some("1 coin · 0.1 XMR"));
        assert!(xmr.footer.starts_with("Network fee about 0.000") && !xmr.footer.contains("old"), "{}", xmr.footer);
        assert_eq!(xmr.button, "Split into 1 coin");
    }

    #[test]
    fn the_rest_of_a_split_reads_like_the_design() {
        let usd = usd();
        let money = Some((&usd, PRICE));
        let notes = notes(money);
        let payments = [XMR / 100; 3];
        let b = budget(2_000);
        let opening = view(&notes, &[], &payments, &b, money, None);
        let auto = opening.auto.unwrap();
        assert_eq!((auto.label.as_str(), auto.spoken.as_str()), ("Auto", "Auto, fill coins like your last 30 days"));
        assert_eq!((opening.title.as_str(), opening.button.as_str()), ("Split up", "Add a coin to split"));
        let fifteen = view(&notes, &[5, 2, 4, 4, 0, 0], &[], &b, money, None);
        assert_eq!(fifteen.summary.as_deref(), Some("15 coins · $325.00"));
        assert_eq!(coins(12), "12 coins");
        assert_eq!(swipe_text(1), "Swipe to split into 1 coin");
        assert_eq!(done_text(12), "12 coins on their way back to this wallet");
        let tiny = SplitFee { fee: FeeAmount::LessThan("$0.01".into()), age: None };
        assert_eq!(fee(&tiny), "Network fee about less than $0.01");
    }
}
