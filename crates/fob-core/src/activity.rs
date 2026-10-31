//! Activity as Home shows it: grouped by day under "TODAY", "YESTERDAY",
//! "27 OCT", each row with its title, one line of status and time, and its
//! amount, so the apps only draw.

use crate::{
    amount,
    wallet::{Activity, Direction, UNLOCK},
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

/// One row of Activity, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub activity: Activity,
    /// The shop, who was paid ("86Ux … 7kQa"), or "Received".
    pub title: String,
    /// Status, receipt and time, no date: "Receipt, 3 items · 09:12",
    /// "Not ready · 3 of 10 · 08:40", "Sent · 10:15".
    pub line: String,
    /// With its sign: "+0.5", "−0.1421".
    pub amount: String,
    /// The receipt's total, when it came with one: "$13.50".
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
/// "Confirming · 3 of 10" (out) or "Not ready · 3 of 10" (in).
fn waiting(activity: &Activity) -> Option<String> {
    let out = activity.direction == Direction::Out;
    let c = activity.confirmations;
    match () {
        _ if activity.height.is_none() => Some(if out { "Sending".to_owned() } else { "On the way".to_owned() }),
        _ if c < UNLOCK => Some(format!("{} · {c} of {UNLOCK}", if out { "Confirming" } else { "Not ready" })),
        _ => None,
    }
}

/// A payment's status on its own screen: as in Activity while it waits,
/// then "Confirmed · 24".
pub fn status(activity: &Activity) -> String {
    waiting(activity).unwrap_or_else(|| format!("Confirmed · {}", activity.confirmations))
}

fn row(activity: Activity, time: i64, clock: Clock) -> Row {
    let out = activity.direction == Direction::Out;
    let title = match (&activity.receipt, &activity.to) {
        (Some(r), _) => r.merchant.clone(),
        (None, Some(to)) if out => short_address(to),
        _ if out => "Sent".into(),
        _ => "Received".into(),
    };
    let status = match waiting(&activity) {
        Some(waiting) => Some(waiting),
        // A receipt says it was a payment; "Sent" would only repeat it.
        None if out && activity.receipt.is_none() => Some("Sent".into()),
        None => None,
    };
    let receipt = activity.receipt.as_ref().map(|r| {
        let items = format!("{} item{}", r.item_count, if r.item_count == 1 { "" } else { "s" });
        if status.is_none() { format!("Receipt, {items}") } else { format!("receipt, {items}") }
    });
    let line = [status, receipt, Some(time_of_day(time, clock))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Row {
        title,
        line,
        amount: format!("{}{}", if out { "−" } else { "+" }, amount::format_short(activity.amount)),
        fiat: activity.receipt.as_ref().map(|r| r.total.clone()),
        activity,
    }
}

/// `rows` (as [`Wallet::activity`](crate::Wallet::activity) gives them)
/// grouped by the phone's calendar day, newest day first; each day keeps the
/// rows' order.
pub fn by_day(rows: Vec<Activity>, clock: Clock) -> Vec<Day> {
    let mut days: Vec<(i64, Day)> = vec![];
    for activity in rows {
        let (day, time) = local(activity.time, clock);
        let row = row(activity, time, clock);
        match days.iter_mut().find(|(d, _)| *d == day) {
            Some((_, group)) => group.rows.push(row),
            None => days.push((day, Day { label: day_label(day, clock), rows: vec![row] })),
        }
    }
    days.sort_by_key(|(day, _)| std::cmp::Reverse(*day));
    days.into_iter().map(|(_, day)| day).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::ReceiptView;

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
            receipt: None,
            receipt_json: None,
            price: None,
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
        let labels: Vec<_> = by_day(rows, c).into_iter().map(|d| d.label).collect();
        assert_eq!(labels, ["TODAY", "YESTERDAY", "27 OCT", "26 SEP 2025"]);
    }

    #[test]
    fn the_phones_offset_decides_the_day() {
        // 23:30 UTC on the 30th is the 31st an hour east.
        let late = NOW - 12 * HOUR - HOUR / 2;
        assert_eq!(by_day(vec![activity(Direction::In, 1, late, 20)], clock(0))[0].label, "YESTERDAY");
        let east = by_day(vec![activity(Direction::In, 1, late, 20)], clock(3600));
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
        let days = by_day(rows, clock(0));
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

        let rows = by_day(vec![sent, paid, arriving, on_the_way, confirming], clock(0)).remove(0).rows;
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
    fn a_twelve_hour_clock_says_am_and_pm() {
        let c = Clock { hours_24: false, ..clock(0) };
        assert_eq!(time_of_day(0, c), "12:00 am");
        assert_eq!(time_of_day(9 * 3600 + 12 * 60, c), "9:12 am");
        assert_eq!(time_of_day(12 * 3600, c), "12:00 pm");
        assert_eq!(time_of_day(22 * 3600 + 5 * 60, c), "10:05 pm");
    }
}
