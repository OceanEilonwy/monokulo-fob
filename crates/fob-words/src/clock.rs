//! Days, in words. A date itself ("27 Oct") comes ready from the clock.

use fob_screens::clock::{Day, When};

/// "Today", "Yesterday", "27 Oct".
pub fn day(day: &Day) -> String {
    match day {
        Day::Today => "Today".into(),
        Day::Yesterday => "Yesterday".into(),
        Day::Date(date) => date.clone(),
    }
}

/// Over a day's rows: "TODAY", "YESTERDAY", "27 OCT".
pub fn heading(of: &Day) -> String {
    day(of).to_uppercase()
}

/// In the middle of a line: "today", "yesterday", "27 Oct".
pub fn in_a_line(of: &Day) -> String {
    match of {
        Day::Date(date) => date.clone(),
        _ => day(of).to_lowercase(),
    }
}

/// "Today 08:40", "Yesterday 08:40", "27 Oct 08:40", "27 Oct 2025 08:40".
pub fn when(when: &When) -> String {
    format!("{} {}", day(&when.day), when.time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_say_their_names() {
        let date = Day::Date("27 Oct 2025".into());
        assert_eq!((day(&Day::Today).as_str(), heading(&Day::Today).as_str(), in_a_line(&Day::Today).as_str()), ("Today", "TODAY", "today"));
        assert_eq!((day(&Day::Yesterday).as_str(), heading(&Day::Yesterday).as_str(), in_a_line(&Day::Yesterday).as_str()), ("Yesterday", "YESTERDAY", "yesterday"));
        assert_eq!((day(&date).as_str(), heading(&date).as_str(), in_a_line(&date).as_str()), ("27 Oct 2025", "27 OCT 2025", "27 Oct 2025"));
        assert_eq!(when(&When { day: Day::Today, time: "08:40".into() }), "Today 08:40");
    }
}
