//! Activity and a payment's own screen, in words.

use fob_monero::fiat::Change;
use fob_screens::activity::{self, Activity, LineStatus, Party, PaidLine, PaymentStatus, Pieces, RowFiat, RowLine, RowTitle};

use crate::{capitalised, clock, split};

/// A row of Activity as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub activity: Option<Activity>,
    pub request: Option<u64>,
    pub share: Option<u64>,
    pub time: u64,
    /// The shop, who was paid ("86Ux … 7kQa"), "From Sam", "Received",
    /// "Request to Sam", "Shared your profile", "Via your profile".
    pub title: String,
    /// Note, status, receipt and time, no date: "Receipt, 3 items · 09:12",
    /// "Pizza · not ready · 3 of 10 · 08:40", "Sent · 10:15".
    pub line: String,
    pub amount: String,
    /// The receipt's total, or what it was worth when it moved: "$83.03";
    /// "asked" under a request's amount.
    pub fiat: Option<String>,
}

pub fn row(r: activity::Row) -> Row {
    Row { activity: r.activity, request: r.request, share: r.share, time: r.time, title: title(&r.title), line: line(&r.line), amount: r.amount, fiat: r.fiat.map(fiat) }
}

/// A day's rows under its label.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    /// "TODAY", "YESTERDAY", "27 OCT", "27 OCT 2025".
    pub label: String,
    pub rows: Vec<Row>,
}

pub fn day(d: activity::Day) -> Day {
    Day { label: clock::heading(&d.label), rows: d.rows.into_iter().map(row).collect() }
}

fn title(title: &RowTitle) -> String {
    match title {
        RowTitle::Name(name) => name.clone(),
        RowTitle::From(name) => format!("From {name}"),
        RowTitle::Split => split::TITLE.into(),
        RowTitle::ToSelf => "Sent to yourself".into(),
        RowTitle::ViaProfile => "Via your profile".into(),
        RowTitle::Sent => "Sent".into(),
        RowTitle::Received => "Received".into(),
        RowTitle::Request(Some(name)) => format!("Request to {name}"),
        RowTitle::Request(None) => "Request".into(),
        RowTitle::Met(name) => format!("Met {name}"),
        RowTitle::SharedProfile => "Shared your profile".into(),
    }
}

fn line(line: &RowLine) -> String {
    let parts: Vec<String> = match line {
        RowLine::Payment { note, shared, status: where_it_is, coins, receipt_items, time } => {
            let said = where_it_is.map(|s| match s {
                LineStatus::Waiting(s) => status(s),
                LineStatus::Sent => "Sent".to_owned(),
            });
            let receipt = receipt_items.map(|n| {
                let items = format!("{n} item{}", if n == 1 { "" } else { "s" });
                if said.is_none() { format!("Receipt, {items}") } else { format!("receipt, {items}") }
            });
            // What a payment in was for comes first, and the status after it in lower case.
            let lead = note.clone().or_else(|| shared.as_ref().map(|when| format!("Shared {when}")));
            let said = if lead.is_some() { said.map(|s| s.to_lowercase()) } else { said };
            [lead, said, coins.map(split::coins), receipt, Some(time.clone())].into_iter().flatten().collect()
        }
        RowLine::Request { named, note, received, time } => {
            let state = received.as_ref().map_or("waiting".to_owned(), |text| format!("received {text}"));
            let parts = [(!named).then(|| "No name".to_owned()), note.clone(), Some(state), Some(time.clone())];
            return capitalised(&parts.into_iter().flatten().collect::<Vec<_>>().join(" · "));
        }
        RowLine::Share { time } => vec!["A new address for this share".to_owned(), time.clone()],
        RowLine::RequestPayment { when, waiting } => [Some(clock::when(when)), waiting.map(|w| status(w).to_lowercase())].into_iter().flatten().collect(),
    };
    parts.join(" · ")
}

fn fiat(fiat: RowFiat) -> String {
    match fiat {
        RowFiat::Worth(text) => text,
        RowFiat::Asked => "asked".into(),
    }
}

/// "Sending", "On the way", "Confirming · 3 of 10" (out), "Not ready · 3 of
/// 10" (in; "20 of 60" for a mined reward), then "Confirmed · 24".
pub fn status(status: PaymentStatus) -> String {
    match status {
        PaymentStatus::Released => "Didn't go through".into(),
        PaymentStatus::NotConfirmed => "Not confirmed".into(),
        PaymentStatus::Sending => "Sending".into(),
        PaymentStatus::OnTheWay => "On the way".into(),
        PaymentStatus::Confirming { done, needed } => format!("Confirming · {done} of {needed}"),
        PaymentStatus::NotReady { done, needed } => format!("Not ready · {done} of {needed}"),
        PaymentStatus::Confirmed { confirmations } => format!("Confirmed · {confirmations}"),
    }
}

/// Under a payment out the node gave no answer for when it was sent, on Paid
/// and on the payment's own screen. Someone told only "Sent" whose payee says
/// nothing came may pay again by hand while Fob still offers this one, and
/// both could arrive; this says why it may yet.
pub const UNANSWERED: &str = "The node didn't answer, so Fob keeps sending it until it does.";

/// The words for where a price can't be had.
const NOT_AVAILABLE: &str = "Not available";

/// A payment's own screen as it's said.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    /// With its sign and every digit: "+0.5", "−0.1421".
    pub amount: String,
    /// Every digit, for the details: "0.142135791113 XMR".
    pub exact: String,
    /// "Today 08:40 · Coffee beans"
    pub when: String,
    /// A default currency besides XMR: the two price boxes show.
    pub priced: bool,
    /// What it's worth now: "$83.03", or "Not available".
    pub worth_now: String,
    /// "1 XMR = $166.06", with a current price.
    pub rate_now: Option<String>,
    /// "When received", "When sent".
    pub then_label: String,
    /// What it was worth then, or "Not available".
    pub worth_then: String,
    pub rate_then: Option<String>,
    /// "Up $3.63 (4.6%) since then", when both values are known.
    pub change: Option<String>,
    /// The change is up (drawn green; down is muted).
    pub up: bool,
    /// "Not ready · 3 of 10", "Confirmed · 24".
    pub status: String,
    /// While it can't be spent or isn't settled: confirmations ÷ needed.
    pub progress: Option<f32>,
    /// "From" or "To".
    pub party_label: String,
    /// "Address 3 · Coffee beans", "Main address", "86Ux … 7kQa".
    pub party: Option<String>,
    /// "Coins" (received) or "Coins spent".
    pub pieces_label: String,
    /// "2 · 0.3 + 0.2"
    pub pieces: Option<String>,
    /// The network fee of a payment out: "0.00003 XMR".
    pub fee: Option<String>,
    /// The transaction id cut to its ends: "a1b2c3 … d4e5f6".
    pub tx_short: String,
    /// "Prices from Kraken.", when a price shows.
    pub footer: Option<String>,
    /// Sent, not confirmed: its coins can be released.
    pub can_release: bool,
}

pub fn detail(d: activity::Detail) -> Detail {
    let note = d.note.as_deref().map(|n| format!(" · {n}")).unwrap_or_default();
    let up = matches!(d.change, Some(Change::Up { .. }));
    Detail {
        amount: d.amount,
        exact: d.exact,
        when: format!("{}{note}", clock::when(&d.when)),
        priced: d.priced,
        worth_now: d.worth_now.unwrap_or_else(|| NOT_AVAILABLE.into()),
        rate_now: d.rate_now.map(|rate| match &d.rate_age {
            Some(age) => format!("{rate} · {}", crate::prices::age(*age)),
            None => rate,
        }),
        then_label: if d.out { "When sent" } else { "When received" }.into(),
        worth_then: d.worth_then.unwrap_or_else(|| NOT_AVAILABLE.into()),
        rate_then: d.rate_then,
        change: d.change.as_ref().map(change),
        up,
        status: status(d.status),
        progress: d.progress,
        party_label: if d.out { "To" } else { "From" }.into(),
        party: d.party.as_ref().map(party),
        pieces_label: if d.out { "Coins spent" } else { "Coins" }.into(),
        pieces: d.pieces.as_ref().map(pieces),
        fee: d.fee,
        tx_short: d.tx_short,
        footer: d.prices_from.map(|provider| format!("Prices from {provider}.")),
        can_release: d.can_release,
    }
}

/// "Up $3.63 (4.6%) since then", "Down $1.20 (0.8%) since then", "No change since then".
pub fn change(change: &Change) -> String {
    let said = |word: &str, by: &str, percent: &Option<String>| match percent {
        Some(percent) => format!("{word} {by} ({percent}%) since then"),
        None => format!("{word} {by} since then"),
    };
    match change {
        Change::Same => "No change since then".into(),
        Change::Up { by, percent } => said("Up", by, percent),
        Change::Down { by, percent } => said("Down", by, percent),
    }
}

fn party(party: &Party) -> String {
    match party {
        Party::ThisWallet => split::TO.into(),
        Party::Named(name) => name.clone(),
        Party::Received { subaddress, contact, via_share, note } => {
            let address = if *subaddress == 0 { "Main address".to_owned() } else { format!("Address {subaddress}") };
            // Paid to a request made for a contact: who it was for first.
            let via = via_share.then(|| "via your profile".to_owned());
            [contact.clone(), Some(address), via, note.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ")
        }
    }
}

/// "2 · 0.3 + 0.2", largest first; past four, "+ 3 more".
fn pieces(pieces: &Pieces) -> String {
    let more = (pieces.more > 0).then(|| format!("{} more", pieces.more));
    format!("{} · {}", pieces.count, pieces.shown.iter().cloned().chain(more).collect::<Vec<_>>().join(" + "))
}

/// Someone paid before, for Send's sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct PaidBefore {
    pub address: String,
    /// The shop on the last payment's receipt, else the address cut to its
    /// ends: "Corner Café", "86Ux … 7kQa".
    pub name: String,
    /// The last payment: its note and day ("Lunch money · 27 Oct"), or its
    /// day and amount ("Today · $24.50", "3 Oct · 0.1421 XMR").
    pub line: String,
}

pub fn paid_before(p: activity::PaidBefore) -> PaidBefore {
    PaidBefore { address: p.address, name: p.name, line: paid_line(&p.line) }
}

/// One line about a payment out.
pub fn paid_line(line: &PaidLine) -> String {
    match line {
        PaidLine::NoteDay { note, day } => format!("{note} · {}", clock::day(day)),
        PaidLine::DayAmount { day, amount } => format!("{} · {amount}", clock::day(day)),
        PaidLine::PaidDayAmount { day, amount } => format!("Paid {} · {amount}", clock::in_a_line(day)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fob_codes::receipt::ReceiptView;
    use fob_screens::{activity::{Activity, Direction, Row as Typed, RowLine, RowTitle}, clock::{self as times, Clock}};
    use fob_monero::fiat;

    fn by_day(rows: Vec<Activity>, requests: Vec<Typed>, clock: Clock) -> Vec<Day> {
        fob_screens::activity::by_day(rows, requests, clock).into_iter().map(day).collect()
    }

    fn detail(activity: &Activity, now: Option<(&fiat::Currency, u64)>, priced: bool, provider: &str, clock: Clock) -> Detail {
        super::detail(fob_screens::activity::detail(activity, now, priced, provider, clock))
    }

    fn status(activity: &Activity) -> String {
        super::status(fob_screens::activity::payment_status(activity))
    }

    fn when(time: u64, clock: Clock) -> String {
        crate::clock::when(&times::when(time, clock))
    }

    fn paid_before(rows: &[Activity], clock: Clock) -> Vec<PaidBefore> {
        fob_screens::activity::paid_before(rows, clock).into_iter().map(super::paid_before).collect()
    }
    use fob_monero::network::UNLOCK;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const HOUR: u64 = 3600;
    const DAY: u64 = 86_400;

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
            receipt_json: None,
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

    fn receipt(items: u32) -> ReceiptView {
        ReceiptView {
            merchant: "Kiosk Café".into(),
            issued: "2026-10-31T09:12:00Z".into(),
            lines: vec![],
            item_count: items,
            tax: None,
            total: "$13.50".into(),
            currency: "USD".into(),
            total_value: 13.5,
        }
    }

    #[test]
    fn days_are_labelled_today_yesterday_then_dates() {
        let c = clock(0);
        let rows = vec![
            activity(Direction::In, 1, NOW - HOUR, 20),
            activity(Direction::In, 1, NOW - DAY, 20),
            activity(Direction::In, 1, NOW - 4 * DAY, 20),
            activity(Direction::In, 1, NOW - 400 * DAY, 20),
        ];
        let labels: Vec<_> = by_day(rows, vec![], c).into_iter().map(|d| d.label).collect();
        assert_eq!(labels, ["TODAY", "YESTERDAY", "27 OCT", "26 SEP 2025"]);
    }

    #[test]
    fn the_phones_offset_decides_the_day() {
        // 23:30 UTC on the 30th is the 31st an hour east.
        let late = NOW - 12 * HOUR - HOUR / 2;
        assert_eq!(by_day(vec![activity(Direction::In, 1, late, 20)], vec![], clock(0))[0].label, "YESTERDAY");
        let east = by_day(vec![activity(Direction::In, 1, late, 20)], vec![], clock(3600));
        assert_eq!(east[0].label, "TODAY");
        assert_eq!(east[0].rows[0].line, "00:30");
    }

    #[test]
    fn rows_of_a_day_stay_together_and_in_order() {
        // An unconfirmed payment from yesterday sorts first in the wallet's
        // list but belongs under its own day.
        let mut pending = activity(Direction::Out, 1, NOW - DAY, 0);
        pending.height = None;
        let rows = vec![pending, activity(Direction::In, 2, NOW - HOUR, 20), activity(Direction::In, 3, NOW - 2 * HOUR, 20)];
        let days = by_day(rows, vec![], clock(0));
        assert_eq!(days.len(), 2);
        assert_eq!((days[0].label.as_str(), days[0].rows.len()), ("TODAY", 2));
        assert_eq!(days[0].rows[0].amount, "+0.000000000002");
        assert_eq!((days[1].label.as_str(), days[1].rows[0].line.as_str()), ("YESTERDAY", "Sending · 12:00"));
    }

    #[test]
    fn lines_read_like_the_designs() {
        let at = |h: u64, m: u64| NOW - 12 * HOUR + h * HOUR + m * 60;
        let mut paid = activity(Direction::Out, 142_100_000_000, at(9, 12), 20);
        paid.receipt = Some(receipt(3));
        let mut sent = activity(Direction::Out, 500_000_000_000, at(10, 15), 20);
        sent.to = Some("86Ux3m9Pqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq7kQa".into());
        let arriving = activity(Direction::In, 653_000_000_000, at(8, 40), 3);
        let mut on_the_way = activity(Direction::In, 1_000_000_000_000, at(11, 5), 0);
        on_the_way.height = None;
        let mut confirming = activity(Direction::Out, 1, at(7, 0), 4);
        confirming.receipt = Some(receipt(1));

        let rows = by_day(vec![sent, paid, arriving, on_the_way, confirming], vec![], clock(0)).remove(0).rows;
        let shown: Vec<_> = rows.iter().map(|r| (r.title.as_str(), r.line.as_str(), r.amount.as_str(), r.fiat.as_deref())).collect();
        // A receipt's "$13.50" is what the payee wrote, so it's not shown as
        // the payment's worth; without a price of Fob's own, nothing is.
        assert_eq!(
            shown,
            [
                ("86Ux … 7kQa", "Sent · 10:15", "−0.5", None),
                ("Kiosk Café", "Receipt, 3 items · 09:12", "−0.1421", None),
                ("Received", "Not ready · 3 of 10 · 08:40", "+0.653", None),
                ("Received", "On the way · 11:05", "+1", None),
                ("Kiosk Café", "Confirming · 4 of 10 · receipt, 1 item · 07:00", "−0.000000000001", None),
            ]
        );
    }

    #[test]
    fn payments_to_a_request_say_who_and_what_for_and_requests_sit_among_them() {
        let at = |h: u64, m: u64| NOW - 12 * HOUR + h * HOUR + m * 60;
        let mut from_sam = activity(Direction::In, 500_000_000_000, at(8, 40), 3);
        from_sam.contact = Some("Sam".into());
        from_sam.note = Some("Concert tickets".into());
        from_sam.subaddress = Some(9);
        let mut unnamed = activity(Direction::In, 1, at(8, 20), 20);
        unnamed.note = Some("Bike".into());
        let mut pending = activity(Direction::In, 2, at(6, 0), 0);
        pending.height = None;
        let cafe = activity(Direction::Out, 3, at(7, 58), 20);
        let request = |id: u64, time: u64| Typed {
            activity: None,
            request: Some(id),
            share: None,
            time,
            title: RowTitle::Request(None),
            line: RowLine::Share { time: String::new() },
            amount: String::new(),
            fiat: None,
        };
        let rows = by_day(vec![pending, from_sam.clone(), unnamed, cafe], vec![request(1, at(8, 30)), request(2, at(8, 12)), request(3, NOW - DAY)], clock(0));
        // A request's own line is pinned with requests.
        let today: Vec<_> = rows[0].rows.iter().map(|r| (r.title.as_str(), if r.request.is_some() { "" } else { r.line.as_str() }, r.request)).collect();
        assert_eq!(
            today,
            [
                // Not yet mined: on top, whatever its time.
                ("Received", "On the way · 06:00", None),
                ("From Sam", "Concert tickets · not ready · 3 of 10 · 08:40", None),
                ("Request", "", Some(1)),
                ("Received", "Bike · 08:20", None),
                ("Request", "", Some(2)),
                ("Sent", "Sent · 07:58", None),
            ]
        );
        assert_eq!((rows[1].label.as_str(), rows[1].rows[0].request), ("YESTERDAY", Some(3)));
        let d = detail(&from_sam, None, false, "Kraken", clock(0));
        assert_eq!(d.party.as_deref(), Some("Sam · Address 9 · Concert tickets"));
    }

    #[test]
    fn a_row_shows_what_it_was_worth_or_its_receipt_s_total() {
        let record = |price| fob_monero::fiat::Record { currency: "USD".into(), price, provider: "Kraken".into(), at: NOW, daily: false };
        let mut priced = activity(Direction::In, 500_000_000_000, NOW - HOUR, 20);
        priced.price = Some(record(166_060_000));
        let mut paid = activity(Direction::Out, 142_100_000_000, NOW - 2 * HOUR, 20);
        paid.receipt = Some(receipt(3));
        paid.price = Some(record(95_000_000));
        let unpriced = activity(Direction::In, 1, NOW - 3 * HOUR, 20);
        let rows = by_day(vec![priced, paid, unpriced], vec![], clock(0)).remove(0).rows;
        let fiat: Vec<_> = rows.iter().map(|r| r.fiat.as_deref()).collect();
        assert_eq!(fiat, [Some("$83.03"), Some("$13.50"), None]);
    }

    #[test]
    fn a_payment_s_own_screen_reads_like_the_designs() {
        let usd = fob_monero::fiat::currency("USD");
        let c = clock(0);
        let at = |h: u64, m: u64| NOW - 12 * HOUR + h * HOUR + m * 60;
        let mut received = activity(Direction::In, 500_000_000_000, at(8, 40), 3);
        received.tx = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90".into();
        received.price = Some(fob_monero::fiat::Record { currency: "USD".into(), price: 158_800_000, provider: "Kraken".into(), at: at(8, 40), daily: false });
        received.subaddress = Some(3);
        received.note = Some("Coffee beans".into());
        received.pieces = vec![200_000_000_000, 300_000_000_000];
        let d = detail(&received, Some((&usd, 166_060_000)), true, "Kraken", c);
        assert_eq!((d.amount.as_str(), d.when.as_str()), ("+0.5", "Today 08:40 · Coffee beans"));
        assert_eq!(d.exact, "0.5 XMR");
        assert_eq!((d.worth_now.as_str(), d.rate_now.as_deref()), ("$83.03", Some("1 XMR = $166.06")));
        assert_eq!((d.then_label.as_str(), d.worth_then.as_str(), d.rate_then.as_deref()), ("When received", "$79.40", Some("1 XMR = $158.80")));
        assert_eq!((d.change.as_deref(), d.up), (Some("Up $3.63 (4.6%) since then"), true));
        assert_eq!((d.status.as_str(), d.progress), ("Not ready · 3 of 10", Some(0.3)));
        assert_eq!((d.party_label.as_str(), d.party.as_deref()), ("From", Some("Address 3 · Coffee beans")));
        assert_eq!((d.pieces_label.as_str(), d.pieces.as_deref()), ("Coins", Some("2 · 0.3 + 0.2")));
        assert_eq!((d.fee, d.tx_short.as_str(), d.footer.as_deref()), (None, "a1b2c3 … 7e8f90", Some("Prices from Kraken.")));

        // Sent days ago, no price recorded, prices now unavailable.
        let mut sent = activity(Direction::Out, 100_000_000_000, NOW - 4 * DAY, 30);
        sent.fee = Some(30_000_000);
        sent.to = Some("86Ux3m9Pqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq7kQa".into());
        sent.pieces = (1..=6).map(|i| i * 100_000_000_000).collect();
        let d = detail(&sent, None, true, "Kraken", c);
        assert_eq!((d.amount.as_str(), d.when.as_str()), ("−0.1", "27 Oct 12:00"));
        assert_eq!((d.worth_now.as_str(), d.worth_then.as_str(), d.then_label.as_str()), ("Not available", "Not available", "When sent"));
        assert_eq!((d.change, d.progress, d.footer), (None, None, None));
        assert_eq!((d.party_label.as_str(), d.party.as_deref()), ("To", Some("86Ux … 7kQa")));
        assert_eq!(d.pieces.as_deref(), Some("6 · 0.6 + 0.5 + 0.4 + 0.3 + 2 more"));
        assert_eq!(d.fee.as_deref(), Some("0.00003 XMR"));
        // Down since then, in muted words.
        let mut fell = received.clone();
        fell.price.as_mut().unwrap().price = 170_000_000;
        let d = detail(&fell, Some((&usd, 166_060_000)), true, "Kraken", c);
        assert_eq!((d.change.as_deref(), d.up), (Some("Down $1.97 (2.3%) since then"), false));
        // Main address, XMR only.
        received.subaddress = Some(0);
        received.note = None;
        let d = detail(&received, None, false, "Kraken", c);
        assert_eq!((d.party.as_deref(), d.priced, d.footer), (Some("Main address"), false, None));
        assert_eq!(when(NOW - 400 * DAY, c), "26 Sep 2025 12:00");
        assert_eq!(when(NOW - DAY, c), "Yesterday 12:00");
    }

    #[test]
    fn changes_since_then_read_as_words() {
        let said = |then, now| change(&fiat::change(then, now, &fiat::currency("USD")));
        assert_eq!(said(7_900, 8_263), "Up $3.63 (4.6%) since then");
        assert_eq!(said(15_000, 14_880), "Down $1.20 (0.8%) since then");
        assert_eq!(said(100, 100), "No change since then");
    }

    #[test]
    fn a_payment_s_own_screen_says_where_it_is() {
        let mut sending = activity(Direction::Out, 1, NOW, 0);
        sending.height = None;
        let mut on_the_way = activity(Direction::In, 1, NOW, 0);
        on_the_way.height = None;
        let shown: Vec<_> = [
            sending,
            on_the_way,
            activity(Direction::Out, 1, NOW, 3),
            activity(Direction::In, 1, NOW, 9),
            activity(Direction::In, 1, NOW, 24),
            activity(Direction::Out, 1, NOW, 10),
        ]
        .iter()
        .map(status)
        .collect();
        assert_eq!(shown, ["Sending", "On the way", "Confirming · 3 of 10", "Not ready · 9 of 10", "Confirmed · 24", "Confirmed · 10"]);
    }

    #[test]
    fn money_locked_longer_stays_not_ready_past_ten() {
        // A mined reward needs 60: at 24 it isn't ready, on its row or its screen.
        let reward = Activity { needed: 60, ..activity(Direction::In, 1, NOW, 24) };
        assert_eq!(status(&reward), "Not ready · 24 of 60");
        let row = by_day(vec![reward.clone()], vec![], clock(0)).remove(0).rows.remove(0);
        assert_eq!(row.line, "Not ready · 24 of 60 · 12:00");
        let d = detail(&reward, None, false, "Kraken", clock(0));
        assert_eq!((d.status.as_str(), d.progress), ("Not ready · 24 of 60", Some(0.4)));
        let ready = Activity { confirmations: 60, ..reward };
        assert_eq!((status(&ready).as_str(), detail(&ready, None, false, "Kraken", clock(0)).progress), ("Confirmed · 60", None));
    }

    #[test]
    fn a_split_reads_as_its_pieces() {
        let split = Activity { split: Some(12), to: Some("5".repeat(95)), fee: Some(150_000_000), ..activity(Direction::Out, 150_000_000, NOW - HOUR, 20) };
        let confirming = Activity { confirmations: 3, ..split.clone() };
        let rows = by_day(vec![split.clone(), confirming], vec![], clock(0)).remove(0).rows;
        let shown: Vec<_> = rows.iter().map(|r| (r.title.as_str(), r.line.as_str(), r.amount.as_str())).collect();
        assert_eq!(shown, [("Split up", "12 coins · 11:00", "−0.0001"), ("Split up", "Confirming · 3 of 10 · 12 coins · 11:00", "−0.0001")]);
        assert_eq!(detail(&split, None, false, "Kraken", clock(0)).party.as_deref(), Some("This wallet"));
    }

    #[test]
    fn paid_before_lists_each_address_once_newest_first() {
        let c = clock(0);
        let address = |end: char| format!("86Ux{}7kQ{end}", "q".repeat(87));
        let paid = |to: String, amount: u64, time: u64| Activity { to: Some(to), ..activity(Direction::Out, amount, time, 20) };
        let mut cafe = paid(address('a'), 142_100_000_000, NOW - HOUR);
        cafe.receipt = Some(receipt(3));
        let mut sam = paid(address('b'), 100_000_000_000, NOW - 4 * DAY);
        sam.note = Some("Lunch money".into());
        let mut mum = paid(address('c'), 300_000_000_000, NOW - 28 * DAY);
        mum.price = Some(fob_monero::fiat::Record { currency: "USD".into(), price: 166_666_666, provider: "Kraken".into(), at: NOW, daily: false });
        // Sam again, earlier: only the last payment to Sam shows.
        let sam_before = paid(address('b'), 1, NOW - 9 * DAY);
        let split = Activity { split: Some(4), ..paid(address('d'), 1, NOW) };
        let received = activity(Direction::In, 1, NOW, 20);
        // A restored wallet's send: no address known.
        let unknown = activity(Direction::Out, 1, NOW, 20);
        let fourth = paid(address('e'), 1_000_000_000, NOW - 400 * DAY);
        let rows = [sam_before, received, mum.clone(), split, sam, unknown, cafe, fourth];
        let shown: Vec<_> = paid_before(&rows, c).into_iter().map(|p| (p.name, p.line)).collect();
        assert_eq!(
            shown,
            [
                // The shop's name from its receipt, but what was paid, not the receipt's $13.50.
                ("Kiosk Café".to_owned(), "Today · 0.1421 XMR".to_owned()),
                ("86Ux … 7kQb".to_owned(), "Lunch money · 27 Oct".to_owned()),
                ("86Ux … 7kQc".to_owned(), "3 Oct · $50.00".to_owned()),
            ]
        );
        // Without a price, the XMR.
        mum.price = None;
        let yesterday = paid(address('e'), 1_000_000_000, NOW - DAY);
        let p = paid_before(&[mum, yesterday], c);
        assert_eq!((p[0].line.as_str(), p[0].address.as_str()), ("Yesterday · 0.001 XMR", address('e').as_str()));
        assert_eq!(p[1].line, "3 Oct · 0.3 XMR");
        assert!(paid_before(&[], c).is_empty());
    }

    #[test]
    fn a_payment_that_never_arrived_says_so() {
        assert_eq!(super::status(PaymentStatus::NotConfirmed), "Not confirmed");
        assert_eq!(super::status(PaymentStatus::Released), "Didn't go through");
    }
}
