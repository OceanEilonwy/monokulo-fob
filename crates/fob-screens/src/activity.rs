//! Activity as Home shows it: grouped by day, each row with what its title
//! and line describe and its amount. `fob-words` says them.

use fob_monero::{address::short_title, amount, fiat};
use crate::clock::{self, Clock, When, day_label, day_name, local, time_of_day, when};
use fob_codes::receipt::ReceiptView;
use std::sync::Arc;

/// One row of Activity, ready to draw: a payment, or a request to be paid
/// (its own line, apart from the payments to it).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The payment; `None` for a request or a share.
    pub activity: Option<Activity>,
    /// The request, by id; `None` for a payment.
    pub request: Option<u64>,
    /// The share of your profile, by id: its own line, no amount.
    pub share: Option<u64>,
    /// When it moved, or the request or share was made.
    pub time: u64,
    pub title: RowTitle,
    pub line: RowLine,
    /// With its sign: "+0.5", "−0.1421"; for a request, what it asked:
    /// "$80.00", "0.5 XMR" (blank for any amount).
    pub amount: String,
    pub fiat: Option<RowFiat>,
}

/// What a row is titled by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowTitle {
    /// The contact paid, the shop on the receipt, or the address cut to its ends.
    Name(String),
    /// Paid to a request made for this contact.
    From(String),
    Split,
    /// Sent to one of this wallet's own addresses.
    ToSelf,
    /// Paid to a share of your profile.
    ViaProfile,
    Sent,
    Received,
    /// A request, for this contact if it's for one.
    Request(Option<String>),
    Met(String),
    SharedProfile,
}

/// Where a payment on a row stands, once it's said at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStatus {
    Waiting(PaymentStatus),
    Sent,
}

/// What a row's line describes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowLine {
    Payment {
        /// What a payment in was for.
        note: Option<String>,
        /// Without a note, when the share it came through was made: "09:15", "22 Oct".
        shared: Option<String>,
        status: Option<LineStatus>,
        /// A split's number of pieces.
        coins: Option<u32>,
        /// The number of items on its receipt.
        receipt_items: Option<u32>,
        time: String,
    },
    Request {
        named: bool,
        note: Option<String>,
        /// What came to it: "$83.03"; `None` while nothing has.
        received: Option<String>,
        time: String,
    },
    Share {
        time: String,
    },
    /// A payment on its request's own screen.
    RequestPayment {
        when: When,
        waiting: Option<PaymentStatus>,
    },
}

/// What's under a row's amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowFiat {
    /// What it was worth when it moved, at Fob's price (never a receipt's
    /// total): "$83.03".
    Worth(String),
    /// Under a request's amount.
    Asked,
}

/// A day's rows, newest first, under its label.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub label: clock::Day,
    pub rows: Vec<Row>,
}

/// Where a payment stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentStatus {
    /// Out, and not in a block yet.
    Sending,
    /// In, and not in a block yet.
    OnTheWay,
    /// Out, and in a block with too few confirmations to count as settled.
    Confirming { done: u64, needed: u64 },
    /// In, and not spendable yet.
    NotReady { done: u64, needed: u64 },
    Confirmed { confirmations: u64 },
    /// Sent, and a day on the node has never heard of it.
    NotConfirmed,
    /// Sent, and its coins released: as far as anyone knows, it didn't go through.
    Released,
}

/// Where `activity` stands.
pub fn payment_status(activity: &Activity) -> PaymentStatus {
    let out = activity.direction == Direction::Out;
    let (done, needed) = (activity.confirmations, activity.needed);
    match () {
        _ if activity.released => PaymentStatus::Released,
        _ if activity.not_confirmed => PaymentStatus::NotConfirmed,
        _ if activity.height.is_none() && out => PaymentStatus::Sending,
        _ if activity.height.is_none() => PaymentStatus::OnTheWay,
        _ if done < needed && out => PaymentStatus::Confirming { done, needed },
        _ if done < needed => PaymentStatus::NotReady { done, needed },
        _ => PaymentStatus::Confirmed { confirmations: done },
    }
}

/// Where a payment stands while it isn't settled.
pub fn waiting(activity: &Activity) -> Option<PaymentStatus> {
    match payment_status(activity) {
        PaymentStatus::Confirmed { .. } => None,
        status => Some(status),
    }
}

/// A payment's own screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    /// With its sign and every digit: "+0.5", "−0.1421".
    pub amount: String,
    /// Every digit, for the details: "0.142135791113 XMR".
    pub exact: String,
    pub when: When,
    pub note: Option<String>,
    pub out: bool,
    /// A default currency besides XMR: the two price boxes show.
    pub priced: bool,
    /// What it's worth now: "$83.03"; `None` with no price to be had.
    pub worth_now: Option<String>,
    /// "1 XMR = $166.06", with a current price.
    pub rate_now: Option<String>,
    /// How old that rate is, once it's missed a refresh.
    pub rate_age: Option<fiat::PriceAge>,
    /// What it was worth then.
    pub worth_then: Option<String>,
    pub rate_then: Option<String>,
    /// When both values are known.
    pub change: Option<fiat::Change>,
    pub status: PaymentStatus,
    /// While it can't be spent or isn't settled: confirmations ÷ needed.
    pub progress: Option<f32>,
    /// `None` when not known (on its way in the pool, or a restored wallet's send).
    pub party: Option<Party>,
    pub pieces: Option<Pieces>,
    /// The network fee of a payment out: "0.00003 XMR".
    pub fee: Option<String>,
    /// A payment out whose fee isn't known (found through a light wallet
    /// server that gives none): its amount includes the fee.
    pub fee_unknown: bool,
    /// The transaction id cut to its ends: "a1b2c3 … d4e5f6".
    pub tx_short: String,
    /// Who prices come from, when a price shows.
    pub prices_from: Option<String>,
    /// Sent, not confirmed: its coins can be released.
    pub can_release: bool,
}

/// Who's on the other end of a payment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Party {
    /// A split: it paid this wallet.
    ThisWallet,
    /// A contact, or the address cut to its ends.
    Named(String),
    /// A payment in, by the address it came to; 0 is the main address.
    Received { subaddress: u32, contact: Option<String>, via_share: bool, note: Option<String> },
}

/// A payment's pieces: how many, the four largest, and how many more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pieces {
    pub count: usize,
    pub shown: Vec<String>,
    pub more: usize,
}

fn pieces(amounts: &[u64]) -> Option<Pieces> {
    if amounts.is_empty() {
        return None;
    }
    let mut sorted = amounts.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    Some(Pieces { count: sorted.len(), shown: sorted.iter().take(4).map(|a| amount::format_short(*a)).collect(), more: sorted.len().saturating_sub(4) })
}

/// A payment's own screen. `now` is the current price shown (none with XMR
/// only or none asked within the hour); `priced` says whether prices show at
/// all; `provider` names where they come from.
pub fn detail(activity: &Activity, now: Option<(&fiat::Currency, u64)>, priced: bool, provider: &str, clock: Clock) -> Detail {
    let out = activity.direction == Direction::Out;
    let worth = |price: u64, currency: &fiat::Currency| fiat::value(activity.amount, price, currency);
    let then = activity.price.as_ref().map(|r| (fiat::currency(&r.currency), r.price));
    let change = match (now, &then) {
        (Some((c, p)), Some((tc, tp))) if c.code == tc.code => {
            Some(fiat::change(fiat::to_minor(activity.amount, *tp, tc), fiat::to_minor(activity.amount, p, c), c))
        }
        _ => None,
    };
    let party = if activity.split.is_some() || activity.to_self {
        Some(Party::ThisWallet)
    } else if out {
        activity.contact.clone().or_else(|| activity.to.as_deref().map(short_title)).map(Party::Named)
    } else {
        activity.subaddress.map(|subaddress| Party::Received { subaddress, contact: activity.contact.clone(), via_share: activity.shared.is_some(), note: activity.note.clone() })
    };
    let shown = priced && (now.is_some() || then.is_some());
    let tx = &activity.tx;
    Detail {
        amount: format!("{}{}", if out { "−" } else { "+" }, amount::format_short(activity.amount)),
        exact: format!("{} XMR", amount::format(activity.amount)),
        when: when(activity.time, clock),
        note: activity.note.clone(),
        out,
        priced,
        worth_now: now.map(|(c, p)| worth(p, c)),
        rate_now: now.map(|(c, p)| fiat::rate(p, c)),
        rate_age: None,
        worth_then: then.as_ref().map(|(c, p)| worth(*p, c)),
        rate_then: then.as_ref().map(|(c, p)| fiat::rate(*p, c)),
        change,
        status: payment_status(activity),
        progress: waiting(activity).map(|_| (activity.confirmations as f32 / activity.needed.max(1) as f32).min(1.0)),
        party,
        pieces: pieces(&activity.pieces),
        fee: activity.fee.filter(|_| out).map(|f| format!("{} XMR", amount::format(f))),
        fee_unknown: out && activity.fee.is_none(),
        tx_short: if tx.len() > 12 { format!("{} … {}", &tx[..6], &tx[tx.len() - 6..]) } else { tx.clone() },
        can_release: activity.not_confirmed,
        prices_from: shown.then(|| provider.to_owned()),
    }
}

/// A payment's row in Activity.
pub(crate) fn payment_row(activity: Activity, clock: Clock) -> Row {
    let (_, time) = local(activity.time, clock);
    let out = activity.direction == Direction::Out;
    let title = match (&activity.receipt, &activity.to) {
        _ if activity.split.is_some() => RowTitle::Split,
        _ if activity.to_self => RowTitle::ToSelf,
        // The name this wallet gave whoever was paid comes first.
        _ if out && activity.contact.is_some() => RowTitle::Name(activity.contact.clone().unwrap_or_default()),
        // Paid to a request made for a contact.
        _ if !out && activity.contact.is_some() => RowTitle::From(activity.contact.clone().unwrap_or_default()),
        // Paid to a share of your profile.
        _ if !out && activity.shared.is_some() => RowTitle::ViaProfile,
        (Some(r), _) => RowTitle::Name(r.merchant.clone()),
        (None, Some(to)) if out => RowTitle::Name(short_title(to)),
        _ if out => RowTitle::Sent,
        _ => RowTitle::Received,
    };
    let status = match waiting(&activity) {
        Some(waiting) => Some(LineStatus::Waiting(waiting)),
        // A receipt says it was a payment, and a split's pieces what it
        // was; "Sent" would only repeat it.
        None if out && activity.receipt.is_none() && activity.split.is_none() && !activity.to_self => Some(LineStatus::Sent),
        None => None,
    };
    // What a payment in was for: its request's note, or the note put in
    // Receive's code, or when the share it came through was made.
    let note = activity.note.clone().filter(|_| !out);
    let shared = activity.shared.filter(|_| !out && note.is_none()).map(|t| clock::short_when(t, clock));
    let line = RowLine::Payment { note, shared, status, coins: activity.split, receipt_items: activity.receipt.as_ref().map(|r| r.item_count), time: time_of_day(time, clock) };
    Row {
        request: None,
        share: None,
        time: activity.time,
        title,
        line,
        amount: format!("{}{}", if out { "−" } else { "+" }, amount::format_short(activity.amount)),
        // What it was worth when it moved, at Fob's own price, as Confirm
        // values it. Never a receipt's total: that's what the payee wrote,
        // so a code charging 5 XMR with a receipt saying "$5.00" would keep
        // the overcharge out of sight on every later look at history. The
        // total stays in the receipt itself.
        fiat: activity.price.as_ref().map(|p| RowFiat::Worth(fiat::value(activity.amount, p.price, &fiat::currency(&p.currency)))),
        activity: Some(activity),
    }
}

/// `rows` (as [`Wallet::activity`](crate::Wallet::activity) gives them)
/// with `requests`' lines (requests' and shares', newest first) among them
/// by time, grouped by the
/// phone's calendar day, newest day first; each day keeps the rows' order.
/// Payments not yet mined stay on top.
pub fn by_day(rows: Vec<Activity>, requests: Vec<Row>, clock: Clock) -> Vec<Day> {
    let mut requests = requests.into_iter().peekable();
    let mut merged = vec![];
    for activity in rows {
        while let Some(request) = requests.next_if(|r| activity.height.is_some() && r.time >= activity.time) {
            merged.push(request);
        }
        merged.push(payment_row(activity, clock));
    }
    merged.extend(requests);
    let mut days: Vec<(i64, Day)> = vec![];
    for row in merged {
        let (day, _) = local(row.time, clock);
        match days.iter_mut().find(|(d, _)| *d == day) {
            Some((_, group)) => group.rows.push(row),
            None => days.push((day, Day { label: day_label(day, clock), rows: vec![row] })),
        }
    }
    days.sort_by_key(|(day, _)| std::cmp::Reverse(*day));
    days.into_iter().map(|(_, day)| day).collect()
}

/// Someone this wallet paid before, for Send's sheet until contacts come:
/// their address, a name and one line about the last payment to them.
#[derive(Clone, Debug, PartialEq)]
pub struct PaidBefore {
    pub address: String,
    /// The shop on the last payment's receipt, else the address cut to its
    /// ends: "Corner Café", "86Ux … 7kQa".
    pub name: String,
    pub line: PaidLine,
}

/// One line about a payment out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaidLine {
    /// Its note and day.
    NoteDay { note: String, day: clock::Day },
    /// Its day and amount.
    DayAmount { day: clock::Day, amount: String },
    /// For an address no contact has: when it was paid and how much.
    PaidDayAmount { day: clock::Day, amount: String },
}

/// How many people Send's sheet lists as paid before.
pub const PAID_BEFORE: usize = 3;

/// What a payment came to, for a line: its worth when sent, else the XMR:
/// "$24.50", "0.1421 XMR". Never a receipt's total: that's what the payee
/// wrote, not what was paid.
pub(crate) fn paid_amount(payment: &Activity) -> String {
    match &payment.price {
        Some(p) => fiat::value(payment.amount, p.price, &fiat::currency(&p.currency)),
        None => format!("{} XMR", amount::format_short(payment.amount)),
    }
}

/// One line about a payment out: its note and day, or its day and amount.
pub(crate) fn last_line(payment: &Activity, clock: Clock) -> PaidLine {
    let day = day_name(payment.time, clock);
    match &payment.note {
        Some(note) => PaidLine::NoteDay { note: note.clone(), day },
        None => PaidLine::DayAmount { day, amount: paid_amount(payment) },
    }
}

/// The people `rows` (as [`Wallet::activity`](crate::Wallet::activity)
/// gives them) paid, each address once, the most recently paid first, at
/// most [`PAID_BEFORE`]. Splits pay this wallet, so they're left out.
pub fn paid_before(rows: &[Activity], clock: Clock) -> Vec<PaidBefore> {
    let mut sent: Vec<(&Activity, &str)> =
        rows.iter().filter(|a| a.direction == Direction::Out && a.split.is_none() && !a.to_self).filter_map(|a| Some((a, a.to.as_deref()?))).collect();
    sent.sort_by_key(|(a, _)| std::cmp::Reverse(a.time));
    let mut seen: Vec<&str> = vec![];
    let mut people = vec![];
    for (last, to) in sent {
        if seen.contains(&to) {
            continue;
        }
        seen.push(to);
        let name = last.contact.clone().or_else(|| last.receipt.as_ref().map(|r| r.merchant.clone())).unwrap_or_else(|| short_title(to));
        people.push(PaidBefore { address: to.to_owned(), name, line: last_line(last, clock) });
        if people.len() == PAID_BEFORE {
            break;
        }
    }
    people
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

/// One row of Activity: a payment in or out, with its receipt if it had one.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    pub tx: String,
    pub direction: Direction,
    /// What moved, without the fee.
    pub amount: u64,
    pub fee: Option<u64>,
    /// Who was paid, when this wallet sent it.
    pub to: Option<String>,
    pub height: Option<u64>,
    pub time: u64,
    pub confirmations: u64,
    /// Confirmations before it's settled: 10, or for a payment in, as many
    /// as its slowest output needs to be spent (60 for a mined reward, more
    /// for one locked to a later block), the not-ready card's rule.
    pub needed: u64,
    /// Read once, when the payment was recorded, and shared: a copy of the
    /// row doesn't copy the receipt.
    pub receipt: Option<Arc<ReceiptView>>,
    /// What one XMR was worth when it moved, in the currency prices are
    /// shown in; `None` with XMR only or no price to be had.
    pub price: Option<fiat::Record>,
    /// The address (by index) a payment in came to; 0 is the main address.
    pub subaddress: Option<u32>,
    /// Its pieces, in atomic units: the outputs received for a payment in,
    /// the outputs spent for one out.
    pub pieces: Vec<u64>,
    /// What it was for: the note in the payment code (Fob's own for its
    /// address, or the payee's description when paying).
    pub note: Option<String>,
    /// A split, with its number of pieces: money paid back to this wallet,
    /// so `amount` is only the fee, what left.
    pub split: Option<u32>,
    /// Sent to one of this wallet's own addresses: what came back is still
    /// this wallet's, so `amount` is only the fee, what left.
    pub to_self: bool,
    /// The contact paid, by name, when this wallet sent it to one of a
    /// contact's addresses; for a payment in, the contact whose request it
    /// paid (unless the request was deleted before it came).
    pub contact: Option<String>,
    /// That contact, by id.
    pub contact_id: Option<u64>,
    /// For a payment in to a share of your profile's address: when that
    /// share was made ("Via your profile", "Shared 09:15").
    pub shared: Option<u64>,
    /// For a payment in: what came to each address it paid, by index, so
    /// one transaction paying two requests credits each its own part.
    pub parts: Vec<(u32, u64)>,
    /// Sent, and a day on the node has never heard of it.
    pub not_confirmed: bool,
    /// Sent, its coins since released.
    pub released: bool,
}

impl Activity {
    /// This payment in as far as it came to the address at `subaddress`:
    /// its part there (all of it when it paid that address alone), or
    /// `None` when it paid that address nothing.
    pub fn part(&self, subaddress: u32) -> Option<Activity> {
        let amount = self.part_amount(subaddress)?;
        Some(Activity { amount, subaddress: Some(subaddress), parts: vec![(subaddress, amount)], ..self.clone() })
    }

    /// How much of this payment in came to the address at `subaddress`, as
    /// [`part`](Activity::part) has it, without copying the payment.
    pub fn part_amount(&self, subaddress: u32) -> Option<u64> {
        if self.direction != Direction::In {
            return None;
        }
        match self.parts.as_slice() {
            [] => (self.subaddress == Some(subaddress)).then_some(self.amount),
            parts => parts.iter().find(|p| p.0 == subaddress).map(|p| p.1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
        use fob_monero::network::UNLOCK;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const HOUR: u64 = 3600;
    fn clock(utc_offset: i32) -> Clock {
        Clock { now: NOW, utc_offset, hours_24: true }
    }

    fn activity(direction: Direction, amount: u64, time: u64, confirmations: u64) -> Activity {
        Activity {
            not_confirmed: false,
            released: false,
            tx: format!("{time}"),
            direction,
            amount,
            fee: None,
            to: None,
            height: Some(100),
            time,
            confirmations,
            needed: UNLOCK,
            receipt: None,
            price: None,
            subaddress: None,
            pieces: vec![],
            note: None,
            split: None,
            to_self: false,
            contact: None,
            contact_id: None,
            shared: None,
            parts: vec![],
        }
    }

    #[test]
    fn a_twelve_hour_clock_says_am_and_pm() {
        let c = Clock { hours_24: false, ..clock(0) };
        assert_eq!(time_of_day(0, c), "12:00 am");
        assert_eq!(time_of_day(9 * 3600 + 12 * 60, c), "9:12 am");
        assert_eq!(time_of_day(12 * 3600, c), "12:00 pm");
        assert_eq!(time_of_day(22 * 3600 + 5 * 60, c), "10:05 pm");
    }

    #[test]
    fn a_payment_says_where_it_stands_without_words() {
        let at = |direction, height: Option<u64>, confirmations| Activity { height, ..activity(direction, 1, NOW, confirmations) };
        let status = |a: &Activity| payment_status(a);
        assert_eq!(status(&at(Direction::Out, None, 0)), PaymentStatus::Sending);
        assert_eq!(status(&at(Direction::In, None, 0)), PaymentStatus::OnTheWay);
        assert_eq!(status(&at(Direction::Out, Some(1), 3)), PaymentStatus::Confirming { done: 3, needed: 10 });
        assert_eq!(status(&at(Direction::In, Some(1), 3)), PaymentStatus::NotReady { done: 3, needed: 10 });
        assert_eq!(status(&at(Direction::In, Some(1), 24)), PaymentStatus::Confirmed { confirmations: 24 });
    }

    #[test]
    fn a_row_says_what_it_is_without_words() {
        let mut sam = activity(Direction::In, 1, NOW - HOUR, 3);
        sam.contact = Some("Sam".into());
        sam.note = Some("Tickets".into());
        let row = payment_row(sam, clock(0));
        assert_eq!(row.title, RowTitle::From("Sam".into()));
        assert_eq!(
            row.line,
            RowLine::Payment {
                note: Some("Tickets".into()),
                shared: None,
                status: Some(LineStatus::Waiting(PaymentStatus::NotReady { done: 3, needed: 10 })),
                coins: None,
                receipt_items: None,
                time: "11:00".into(),
            }
        );
        let split = Activity { split: Some(12), ..activity(Direction::Out, 1, NOW, 20) };
        let row = payment_row(split, clock(0));
        assert_eq!(row.title, RowTitle::Split);
        assert!(matches!(row.line, RowLine::Payment { status: None, coins: Some(12), .. }));
    }

    #[test]
    fn a_row_is_worth_what_fob_valued_it_at_never_the_receipt_s_total() {
        let receipt = ReceiptView {
            merchant: "Corner Café".into(),
            issued: String::new(),
            lines: vec![],
            item_count: 1,
            tax: None,
            total: "$5.00".into(),
            currency: "USD".into(),
            total_value: 5.0,
        };
        // 5 XMR at $160 is $800, whatever the payee's receipt said.
        let price = fiat::Record { currency: "USD".into(), price: 160 * fiat::MICRO, provider: "Kraken".into(), at: NOW, daily: false };
        let paid = Activity { receipt: Some(Arc::new(receipt)), price: Some(price), ..activity(Direction::Out, 5 * amount::ATOMIC_PER_XMR, NOW, 20) };
        let row = payment_row(paid.clone(), clock(0));
        assert_eq!(row.fiat, Some(RowFiat::Worth("$800.00".into())));
        assert_eq!(paid_amount(&paid), "$800.00");
        // Without a price of Fob's own, nothing is shown, not the receipt's total.
        assert_eq!(payment_row(Activity { price: None, ..paid }, clock(0)).fiat, None);
    }

    #[test]
    fn a_payment_gone_quiet_stands_above_how_far_it_got() {
        let sent = activity(Direction::Out, 5, 100, 0);
        assert_eq!(payment_status(&Activity { not_confirmed: true, height: None, ..sent.clone() }), PaymentStatus::NotConfirmed);
        assert_eq!(payment_status(&Activity { not_confirmed: true, released: true, ..sent }), PaymentStatus::Released);
    }
}
