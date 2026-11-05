//! The device's clock, which only the app knows, and dates and times written
//! the way Fob writes them.

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

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// Days since the epoch on the phone's calendar, and seconds into that day.
pub fn local(time: u64, clock: Clock) -> (i64, i64) {
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

pub(crate) fn day_label(day: i64, clock: Clock) -> String {
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

pub fn time_of_day(seconds: i64, clock: Clock) -> String {
    let (h, m) = (seconds / 3600, seconds % 3600 / 60);
    if clock.hours_24 {
        format!("{h:02}:{m:02}")
    } else {
        let half = if h < 12 { "am" } else { "pm" };
        format!("{}:{m:02} {half}", if h % 12 == 0 { 12 } else { h % 12 })
    }
}

/// A day that isn't today or yesterday: "27 Oct", "27 Oct 2025".
pub fn date(day: i64, today: i64) -> String {
    let (year, month, d) = civil(day);
    let name = month_name(month);
    if year == civil(today).0 { format!("{d} {name}") } else { format!("{d} {name} {year}") }
}

/// A day with its year whatever the year: "2 Sep 2026".
pub fn full_date(day: i64) -> String {
    let (year, month, d) = civil(day);
    format!("{d} {} {year}", month_name(month))
}

fn month_name(month: usize) -> String {
    let name = &MONTHS[month - 1];
    format!("{}{}", &name[..1], name[1..].to_lowercase())
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_793_448_000;

    fn clock(utc_offset: i32) -> Clock {
        Clock { now: NOW, utc_offset, hours_24: true }
    }

    #[test]
    fn the_clock_is_where_the_designs_say() {
        assert_eq!(civil(local(NOW, clock(0)).0), (2026, 10, 31));
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }
}
