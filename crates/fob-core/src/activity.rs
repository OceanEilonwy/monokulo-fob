//! Activity as Home shows it: grouped by day under "TODAY", "YESTERDAY",
//! "27 OCT", each row with its title, one line of status and time, and its
//! amount, so the apps only draw.

use crate::{
    amount, price, split,
    wallet::{Activity, Direction},
};

/// The phone's clock, which only the app knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Seconds since the epoch.
    pub now: u64,
    /// The phone's offset from UTC now, in seconds ("+01:00" is 3600).
    pub utc_offset: i32,
    /// "09:12" rather than "9:12 am".
    pub hours_24: bool,
}

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
    /// The shop, who was paid ("86Ux … 7kQa"), "From Sam", "Received",
    /// "Request to Sam", "Shared your profile", "Via your profile".
    pub title: String,
    /// Note, status, receipt and time, no date: "Receipt, 3 items · 09:12",
    /// "Pizza · not ready · 3 of 10 · 08:40", "Sent · 10:15".
    pub line: String,
    /// With its sign: "+0.5", "−0.1421"; for a request, what it asked:
    /// "$80.00", "0.5 XMR" (blank for any amount).
    pub amount: String,
    /// The receipt's total, when it came with one: "$13.50"; otherwise what
    /// it was worth when it moved, when that's known: "$83.03"; "asked" under
    /// a request's amount.
    pub fiat: Option<String>,
}

/// A day's rows, newest first, under its label.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    /// "TODAY", "YESTERDAY", "27 OCT", "27 OCT 2025".
    pub label: String,
    pub rows: Vec<Row>,
}

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// Days since the epoch on the phone's calendar, and seconds into that day.
pub(crate) fn local(time: u64, clock: Clock) -> (i64, i64) {
    let t = time as i64 + i64::from(clock.utc_offset);
    (t.div_euclid(86_400), t.rem_euclid(86_400))
}

/// The civil date of a day since the epoch (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, usize, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m as usize, d)
}

fn day_label(day: i64, clock: Clock) -> String {
    let (today, _) = local(clock.now, clock);
    if day == today {
        return "TODAY".into();
    }
    if day == today - 1 {
        return "YESTERDAY".into();
    }
    let (year, month, d) = civil(day);
    let (this_year, _, _) = civil(today);
    let date = format!("{d} {}", MONTHS[month - 1]);
    if year == this_year { date } else { format!("{date} {year}") }
}

pub(crate) fn time_of_day(seconds: i64, clock: Clock) -> String {
    let (h, m) = (seconds / 3600, seconds % 3600 / 60);
    if clock.hours_24 {
        format!("{h:02}:{m:02}")
    } else {
        let half = if h < 12 { "am" } else { "pm" };
        format!("{}:{m:02} {half}", if h % 12 == 0 { 12 } else { h % 12 })
    }
}

/// An address cut to its ends, for a title: "86Ux … 7kQa".
pub fn short_address(address: &str) -> String {
    let n = address.len();
    if n <= 8 { address.to_owned() } else { format!("{} … {}", &address[..4], &address[n - 4..]) }
}

/// While a payment isn't settled: "Sending", "On the way",
/// "Confirming · 3 of 10" (out) or "Not ready · 3 of 10" (in; "20 of 60"
/// for a mined reward, as many as its outputs need to be spent).
pub(crate) fn waiting(activity: &Activity) -> Option<String> {
    let out = activity.direction == Direction::Out;
    let (c, needed) = (activity.confirmations, activity.needed);
    match () {
        _ if activity.height.is_none() => Some(if out { "Sending".to_owned() } else { "On the way".to_owned() }),
        _ if c < needed => Some(format!("{} · {c} of {needed}", if out { "Confirming" } else { "Not ready" })),
        _ => None,
    }
}

/// A payment's status on its own screen: as in Activity while it waits,
/// then "Confirmed · 24".
pub fn status(activity: &Activity) -> String {
    waiting(activity).unwrap_or_else(|| format!("Confirmed · {}", activity.confirmations))
}

/// A day that isn't today or yesterday: "27 Oct", "27 Oct 2025".
fn date(day: i64, today: i64) -> String {
    let (year, month, d) = civil(day);
    let name = &MONTHS[month - 1];
    let name = format!("{}{}", &name[..1], name[1..].to_lowercase());
    if year == civil(today).0 { format!("{d} {name}") } else { format!("{d} {name} {year}") }
}

/// When, for a payment's own screen: "Today 08:40", "Yesterday 08:40",
/// "27 Oct 08:40", "27 Oct 2025 08:40".
pub fn when(time: u64, clock: Clock) -> String {
    let (day, seconds) = local(time, clock);
    let (today, _) = local(clock.now, clock);
    let date = match day {
        _ if day == today => "Today".to_owned(),
        _ if day == today - 1 => "Yesterday".to_owned(),
        _ => date(day, today),
    };
    format!("{date} {}", time_of_day(seconds, clock))
}

/// When, short, for a line under something else: the time today ("08:40"),
/// the date before ("22 Oct", "22 Oct 2025").
pub(crate) fn short_when(time: u64, clock: Clock) -> String {
    let (day, seconds) = local(time, clock);
    let (today, _) = local(clock.now, clock);
    if day == today { time_of_day(seconds, clock) } else { date(day, today) }
}

/// The words for "Not available", where a price can't be had.
pub const NOT_AVAILABLE: &str = "Not available";

/// A payment's own screen, every word ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    /// With its sign and every digit: "+0.5", "−0.1421".
    pub amount: String,
    /// "Today 08:40 · Coffee beans"
    pub when: String,
    /// Prices are on: the two price boxes show.
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
    /// "Address 3 · Coffee beans", "Main address", "86Ux … 7kQa"; `None`
    /// when not known (on its way in the pool, or a restored wallet's send).
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
}

/// Pieces as "2 · 0.3 + 0.2", largest first; past four, "+ 3 more".
fn pieces(amounts: &[u64]) -> Option<String> {
    if amounts.is_empty() {
        return None;
    }
    let mut sorted = amounts.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let mut parts: Vec<String> = sorted.iter().take(4).map(|a| amount::format_short(*a)).collect();
    if sorted.len() > 4 {
        parts.push(format!("{} more", sorted.len() - 4));
    }
    Some(format!("{} · {}", sorted.len(), parts.join(" + ")))
}

/// The words of a payment's own screen. `now` is the current price shown
/// (none with prices off or none asked within the hour); `prices_on` says
/// whether prices show at all; `provider` names where they come from.
pub fn detail(activity: &Activity, now: Option<(&price::Currency, u64)>, prices_on: bool, provider: &str, clock: Clock) -> Detail {
    let out = activity.direction == Direction::Out;
    let worth = |price: u64, currency: &price::Currency| price::value(activity.amount, price, currency);
    let then = activity.price.as_ref().map(|r| (price::currency(&r.currency), r.price));
    let change = match (now, &then) {
        (Some((c, p)), Some((tc, tp))) if c.code == tc.code => {
            Some(price::change(price::to_minor(activity.amount, *tp, tc), price::to_minor(activity.amount, p, c), c))
        }
        _ => None,
    };
    let party = if activity.split.is_some() {
        Some("This wallet".to_owned())
    } else if out {
        activity.contact.clone().or_else(|| activity.to.as_deref().map(short_address))
    } else {
        activity.subaddress.map(|i| {
            let address = if i == 0 { "Main address".to_owned() } else { format!("Address {i}") };
            // Paid to a request made for a contact: who it was for first.
            let address = match &activity.contact {
                Some(name) => format!("{name} · {address}"),
                None => address,
            };
            // Paid to a share's address.
            let address = if activity.shared.is_some() { format!("{address} · via your profile") } else { address };
            match &activity.note {
                Some(note) => format!("{address} · {note}"),
                None => address,
            }
        })
    };
    let note = activity.note.as_deref().map(|n| format!(" · {n}")).unwrap_or_default();
    let shown = prices_on && (now.is_some() || then.is_some());
    let tx = &activity.tx;
    Detail {
        amount: format!("{}{}", if out { "−" } else { "+" }, amount::format(activity.amount)),
        when: format!("{}{note}", when(activity.time, clock)),
        priced: prices_on,
        worth_now: now.map(|(c, p)| worth(p, c)).unwrap_or_else(|| NOT_AVAILABLE.into()),
        rate_now: now.map(|(c, p)| price::rate(p, c)),
        then_label: if out { "When sent" } else { "When received" }.into(),
        worth_then: then.as_ref().map(|(c, p)| worth(*p, c)).unwrap_or_else(|| NOT_AVAILABLE.into()),
        rate_then: then.as_ref().map(|(c, p)| price::rate(*p, c)),
        up: change.as_ref().is_some_and(|c| c.1),
        change: change.map(|c| c.0),
        status: status(activity),
        progress: waiting(activity).map(|_| (activity.confirmations as f32 / activity.needed.max(1) as f32).min(1.0)),
        party_label: if out { "To" } else { "From" }.into(),
        party,
        pieces_label: if out { "Coins spent" } else { "Coins" }.into(),
        pieces: pieces(&activity.pieces),
        fee: activity.fee.filter(|_| out).map(|f| format!("{} XMR", amount::format(f))),
        tx_short: if tx.len() > 12 { format!("{} … {}", &tx[..6], &tx[tx.len() - 6..]) } else { tx.clone() },
        footer: shown.then(|| format!("Prices from {provider}.")),
    }
}

/// A payment's row in Activity.
pub(crate) fn payment_row(activity: Activity, clock: Clock) -> Row {
    let (_, time) = local(activity.time, clock);
    let out = activity.direction == Direction::Out;
    let title = match (&activity.receipt, &activity.to) {
        _ if activity.split.is_some() => split::TITLE.into(),
        // The name this wallet gave whoever was paid comes first.
        _ if out && activity.contact.is_some() => activity.contact.clone().unwrap_or_default(),
        // Paid to a request made for a contact.
        _ if !out && activity.contact.is_some() => format!("From {}", activity.contact.as_deref().unwrap_or_default()),
        // Paid to a share of your profile.
        _ if !out && activity.shared.is_some() => crate::profile::VIA.into(),
        (Some(r), _) => r.merchant.clone(),
        (None, Some(to)) if out => short_address(to),
        _ if out => "Sent".into(),
        _ => "Received".into(),
    };
    let status = match waiting(&activity) {
        Some(waiting) => Some(waiting),
        // A receipt says it was a payment, and a split's pieces what it
        // was; "Sent" would only repeat it.
        None if out && activity.receipt.is_none() && activity.split.is_none() => Some("Sent".into()),
        None => None,
    };
    let pieces = activity.split.map(split::pieces_text);
    let receipt = activity.receipt.as_ref().map(|r| {
        let items = format!("{} item{}", r.item_count, if r.item_count == 1 { "" } else { "s" });
        if status.is_none() { format!("Receipt, {items}") } else { format!("receipt, {items}") }
    });
    // What a payment in was for (its request's note, or the note put in
    // Receive's code, or when the share it came through was made) comes
    // first, and the status after it in lower case.
    let note = activity.note.clone().or_else(|| activity.shared.map(|t| crate::profile::shared_text(t, clock))).filter(|_| !out);
    let status = if note.is_some() { status.map(|s| s.to_lowercase()) } else { status };
    let line = [note, status, pieces, receipt, Some(time_of_day(time, clock))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Row {
        request: None,
        share: None,
        time: activity.time,
        title,
        line,
        amount: format!("{}{}", if out { "−" } else { "+" }, amount::format_short(activity.amount)),
        // A receipt's own total; otherwise what it was worth when it moved.
        fiat: match (&activity.receipt, &activity.price) {
            (Some(r), _) => Some(r.total.clone()),
            (None, Some(p)) => Some(price::value(activity.amount, p.price, &price::currency(&p.currency))),
            (None, None) => None,
        },
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
    /// The last payment: its note and day ("Lunch money · 27 Oct"), or its
    /// day and amount ("Today · $24.50", "3 Oct · 0.1421 XMR").
    pub line: String,
}

/// How many people Send's sheet lists as paid before.
pub const PAID_BEFORE: usize = 3;

/// What a payment came to, for a line: its worth when sent, else the XMR:
/// "$24.50", "0.1421 XMR". Never a receipt's total: that's what the payee
/// wrote, not what was paid.
pub(crate) fn paid_amount(payment: &Activity) -> String {
    match &payment.price {
        Some(p) => price::value(payment.amount, p.price, &price::currency(&p.currency)),
        None => format!("{} XMR", amount::format_short(payment.amount)),
    }
}

/// One line about a payment out: its note and day ("Lunch money · 27
/// Oct"), or its day and amount ("Today · $24.50").
pub(crate) fn last_line(payment: &Activity, clock: Clock) -> String {
    let day = day_name(payment.time, clock);
    match &payment.note {
        Some(note) => format!("{note} · {day}"),
        None => format!("{day} · {}", paid_amount(payment)),
    }
}

/// The day of `time`, for a line: "Today", "Yesterday", "27 Oct".
pub(crate) fn day_name(time: u64, clock: Clock) -> String {
    let (day, _) = local(time, clock);
    let (today, _) = local(clock.now, clock);
    match day {
        _ if day == today => "Today".to_owned(),
        _ if day == today - 1 => "Yesterday".to_owned(),
        _ => date(day, today),
    }
}

/// The people `rows` (as [`Wallet::activity`](crate::Wallet::activity)
/// gives them) paid, each address once, the most recently paid first, at
/// most [`PAID_BEFORE`]. Splits pay this wallet, so they're left out.
pub fn paid_before(rows: &[Activity], clock: Clock) -> Vec<PaidBefore> {
    let mut sent: Vec<(&Activity, &str)> =
        rows.iter().filter(|a| a.direction == Direction::Out && a.split.is_none()).filter_map(|a| Some((a, a.to.as_deref()?))).collect();
    sent.sort_by_key(|(a, _)| std::cmp::Reverse(a.time));
    let mut seen: Vec<&str> = vec![];
    let mut people = vec![];
    for (last, to) in sent {
        if seen.contains(&to) {
            continue;
        }
        seen.push(to);
        let name = last.contact.clone().or_else(|| last.receipt.as_ref().map(|r| r.merchant.clone())).unwrap_or_else(|| short_address(to));
        people.push(PaidBefore { address: to.to_owned(), name, line: last_line(last, clock) });
        if people.len() == PAID_BEFORE {
            break;
        }
    }
    people
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::ReceiptView;
    use crate::wallet::UNLOCK;

    /// 2026-10-31 12:00 UTC.
    const NOW: u64 = 1_793_448_000;
    const HOUR: u64 = 3600;
    const DAY: u64 = 86_400;

    fn clock(utc_offset: i32) -> Clock {
        Clock { now: NOW, utc_offset, hours_24: true }
    }

    fn activity(direction: Direction, amount: u64, time: u64, confirmations: u64) -> Activity {
        Activity {
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
        }
    }

    #[test]
    fn the_clock_is_where_the_designs_say() {
        assert_eq!(civil(local(NOW, clock(0)).0), (2026, 10, 31));
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(19_782), (2024, 2, 29));
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
        assert_eq!(
            shown,
            [
                ("86Ux … 7kQa", "Sent · 10:15", "−0.5", None),
                ("Kiosk Café", "Receipt, 3 items · 09:12", "−0.1421", Some("$13.50")),
                ("Received", "Not ready · 3 of 10 · 08:40", "+0.653", None),
                ("Received", "On the way · 11:05", "+1", None),
                ("Kiosk Café", "Confirming · 4 of 10 · receipt, 1 item · 07:00", "−0.000000000001", Some("$13.50")),
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
        let request = |id: u64, time: u64| Row {
            activity: None,
            request: Some(id),
            share: None,
            time,
            title: "Request".into(),
            line: String::new(),
            amount: String::new(),
            fiat: None,
        };
        let rows = by_day(vec![pending, from_sam.clone(), unnamed, cafe], vec![request(1, at(8, 30)), request(2, at(8, 12)), request(3, NOW - DAY)], clock(0));
        let today: Vec<_> = rows[0].rows.iter().map(|r| (r.title.as_str(), r.line.as_str(), r.request)).collect();
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
        let record = |price| crate::price::Record { currency: "USD".into(), price, provider: "Kraken".into(), at: NOW, daily: false };
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
        let usd = crate::price::currency("USD");
        let c = clock(0);
        let at = |h: u64, m: u64| NOW - 12 * HOUR + h * HOUR + m * 60;
        let mut received = activity(Direction::In, 500_000_000_000, at(8, 40), 3);
        received.tx = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90".into();
        received.price = Some(crate::price::Record { currency: "USD".into(), price: 158_800_000, provider: "Kraken".into(), at: at(8, 40), daily: false });
        received.subaddress = Some(3);
        received.note = Some("Coffee beans".into());
        received.pieces = vec![200_000_000_000, 300_000_000_000];
        let d = detail(&received, Some((&usd, 166_060_000)), true, "Kraken", c);
        assert_eq!((d.amount.as_str(), d.when.as_str()), ("+0.5", "Today 08:40 · Coffee beans"));
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
        // Main address, prices off.
        received.subaddress = Some(0);
        received.note = None;
        let d = detail(&received, None, false, "Kraken", c);
        assert_eq!((d.party.as_deref(), d.priced, d.footer), (Some("Main address"), false, None));
        assert_eq!(when(NOW - 400 * DAY, c), "26 Sep 2025 12:00");
        assert_eq!(when(NOW - DAY, c), "Yesterday 12:00");
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
    fn a_twelve_hour_clock_says_am_and_pm() {
        let c = Clock { hours_24: false, ..clock(0) };
        assert_eq!(time_of_day(0, c), "12:00 am");
        assert_eq!(time_of_day(9 * 3600 + 12 * 60, c), "9:12 am");
        assert_eq!(time_of_day(12 * 3600, c), "12:00 pm");
        assert_eq!(time_of_day(22 * 3600 + 5 * 60, c), "10:05 pm");
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
        mum.price = Some(crate::price::Record { currency: "USD".into(), price: 166_666_666, provider: "Kraken".into(), at: NOW, daily: false });
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
}
