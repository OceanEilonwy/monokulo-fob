//! The price toast: whether a failed lookup is shown, and what it says.
//!
//! Prices are looked up by the 5-minute refresh while Fob is on screen
//! ([`Asked::Background`]) and for something the user did: pull to
//! refresh, a changed currency or provider, Retry ([`Asked::User`]), or a
//! money amount typed on Send or Receive ([`Asked::Typing`]). A failure
//! the user caused always shows; the background's shows once per run of
//! failures, not again every 5 minutes until a lookup has worked. A lookup
//! that works ends the run and takes the toast down.
//!
//! The app shows the toast whose `id` it hasn't shown (or had swiped
//! away) yet: a new id is a new showing, the same id only newer words.

use super::{Cache, Current, Failure, Settings, currency, rate};
use crate::activity::{self, Clock};

/// Who asked for a price.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// The refresh every 5 minutes while Fob is on screen.
    Background,
    /// Pull to refresh, a changed currency or provider, Retry.
    User,
    /// A money amount typed on Send or Receive, wanting a current price.
    Typing,
}

/// A lookup that failed: who failed, how, for which currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub failure: Failure,
    /// "Kraken"
    pub provider: String,
    /// "USD"
    pub currency: String,
    /// The day (UTC, since the epoch) of the history asked for, when only
    /// the history failed: the newest a payment lacks a price for.
    pub day: Option<i64>,
}

/// The failure the toast is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// New for each showing.
    pub id: u64,
    pub failed: Failed,
    /// Asked for by a money amount typed: the words say to type XMR.
    pub typing: bool,
}

/// The run of failures, kept while Fob runs (not in the wallet file).
#[derive(Debug, Default)]
pub struct Alerts {
    alert: Option<Alert>,
    ids: u64,
}

impl Alerts {
    /// After a lookup `asked` for: `None` (it worked) ends the run; a
    /// failure shows (a new id) when the user asked or it starts a run,
    /// and only updates the words when the background meets it again.
    pub fn looked_up(&mut self, failed: Option<Failed>, asked: Asked) {
        let Some(failed) = failed else {
            self.alert = None;
            return;
        };
        let id = match &self.alert {
            Some(a) if asked == Asked::Background => a.id,
            _ => {
                self.ids += 1;
                self.ids
            }
        };
        self.alert = Some(Alert { id, failed, typing: asked == Asked::Typing });
    }

    /// Forgets the run: the price settings changed, so it was about
    /// settings gone.
    pub fn reset(&mut self) {
        self.alert = None;
    }

    /// The failure in this run, if one is going on.
    pub fn alert(&self) -> Option<&Alert> {
        self.alert.as_ref()
    }
}

/// The toast, every word ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    /// "Couldn't get the USD price", "Couldn't get 29 Oct's USD price".
    pub title: String,
    /// "Kraken didn't answer · using the 14:02 price", "No price yet ·
    /// amounts stay in XMR", "CoinGecko is busy · shown as not available".
    pub line: String,
    /// While Retry asks: "Trying again…", "Asking Kraken for the USD price".
    pub retrying_title: String,
    pub retrying_line: String,
}

/// Who failed and how: "Kraken didn't answer", "CoinGecko is busy".
pub fn reason(failure: Failure, provider: &str) -> String {
    match failure {
        Failure::NotAnswering => format!("{provider} didn't answer"),
        Failure::Busy => format!("{provider} is busy"),
        Failure::BadAnswer => format!("{provider} answered without a price"),
        Failure::TorNotYet => "Tor isn't built yet".into(),
    }
}

/// The toast for `alert`, with `last` the current price kept for the
/// settings, however old, and `clock` the phone's.
pub fn toast(alert: &Alert, last: Option<&Current>, clock: Clock) -> Toast {
    let f = &alert.failed;
    let (today, _) = activity::local(clock.now, clock);
    // "USD price", "29 Oct's USD price".
    let what = match f.day {
        Some(day) => format!("{}'s {} price", activity::date(day, today), f.currency),
        None => format!("the {} price", f.currency),
    };
    let reason = reason(f.failure, &f.provider);
    let xmr = if alert.typing { "type the amount in XMR" } else { "amounts stay in XMR" };
    let line = match last {
        _ if f.day.is_some() => format!("{reason} · shown as not available"),
        Some(c) if clock.now.saturating_sub(c.at) < super::SHOWN => format!("{reason} · using {}", price_from(c.at, clock, true)),
        // One too old to use: the reason, and what that means.
        Some(_) => format!("{reason} · {xmr}"),
        None => format!("No price yet · {xmr}"),
    };
    Toast {
        id: alert.id,
        title: format!("Couldn't get {what}"),
        line,
        retrying_title: "Trying again…".into(),
        retrying_line: format!("Asking {} for {what}", f.provider),
    }
}

/// When a price is from: "14:02", "yesterday's 14:02", "29 Oct's" (with
/// `the`, "the 14:02 price"), or "14:02", "yesterday 14:02", "29 Oct".
fn price_from(at: u64, clock: Clock, the: bool) -> String {
    let (day, seconds) = activity::local(at, clock);
    let (today, _) = activity::local(clock.now, clock);
    let time = activity::time_of_day(seconds, clock);
    match (day, the) {
        _ if day == today && the => format!("the {time} price"),
        _ if day == today => time,
        _ if day == today - 1 && the => format!("yesterday's {time} price"),
        _ if day == today - 1 => format!("yesterday {time}"),
        (_, true) => format!("{}'s price", activity::date(day, today)),
        (_, false) => activity::date(day, today),
    }
}

/// Where a price converts an amount, how old it is once it isn't just
/// asked: "price from 14:02"; `None` while it's newer than
/// [`AGED`](super::AGED).
pub fn age(at: u64, clock: Clock) -> Option<String> {
    (clock.now.saturating_sub(at) >= super::AGED).then(|| format!("price from {}", price_from(at, clock, false)))
}

/// Retry's toast when it worked: "USD price updated", "1 XMR = $166.10".
pub fn updated(cache: &Cache, settings: &Settings, now: u64) -> (String, String) {
    match settings.code() {
        Some(code) => {
            let line = cache.shown(settings, now).map(|c| rate(c.price, &currency(&c.currency))).unwrap_or_default();
            (format!("{code} price updated"), line)
        }
        None => ("Prices are off".into(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(failure: Failure) -> Failed {
        Failed { failure, provider: "Kraken".into(), currency: "USD".into(), day: None }
    }

    #[test]
    fn the_background_shows_once_per_run_of_failures() {
        let mut alerts = Alerts::default();
        alerts.looked_up(Some(failed(Failure::NotAnswering)), Asked::Background);
        let first = alerts.alert().unwrap().id;
        // Every 5 minutes after: the same showing, newer words.
        alerts.looked_up(Some(failed(Failure::Busy)), Asked::Background);
        let again = alerts.alert().unwrap();
        assert_eq!((again.id, again.failed.failure), (first, Failure::Busy));
        // A lookup that works ends the run; the next failure shows again.
        alerts.looked_up(None, Asked::Background);
        assert_eq!(alerts.alert(), None);
        alerts.looked_up(Some(failed(Failure::NotAnswering)), Asked::Background);
        assert!(alerts.alert().unwrap().id > first);
    }

    #[test]
    fn what_the_user_did_always_shows() {
        let mut alerts = Alerts::default();
        alerts.looked_up(Some(failed(Failure::NotAnswering)), Asked::Background);
        let mut last = alerts.alert().unwrap().id;
        for asked in [Asked::User, Asked::Typing, Asked::User] {
            alerts.looked_up(Some(failed(Failure::NotAnswering)), asked);
            let alert = alerts.alert().unwrap();
            assert!(alert.id > last, "{asked:?} shows again");
            assert_eq!(alert.typing, asked == Asked::Typing);
            last = alert.id;
        }
        // A settings change forgets the run.
        alerts.reset();
        assert_eq!(alerts.alert(), None);
    }

    fn clock(now: u64) -> Clock {
        Clock { now, utc_offset: 0, hours_24: true }
    }

    #[test]
    fn the_toast_says_what_failed_and_what_fob_uses_instead() {
        // 31 Oct 2026, 15:00 UTC.
        let now = 1_793_458_800;
        let at_1402 = now - 58 * 60;
        let alert = |failure, day, typing| Alert { id: 1, failed: Failed { day, ..failed(failure) }, typing };
        let kept = |at| Current { currency: "USD".into(), provider: "Kraken".into(), price: 166_100_000, at };
        let t = toast(&alert(Failure::NotAnswering, None, false), Some(&kept(at_1402)), clock(now));
        assert_eq!((t.title.as_str(), t.line.as_str()), ("Couldn't get the USD price", "Kraken didn't answer · using the 14:02 price"));
        assert_eq!((t.retrying_title.as_str(), t.retrying_line.as_str()), ("Trying again…", "Asking Kraken for the USD price"));
        // Yesterday's, still within the day.
        let t = toast(&alert(Failure::Busy, None, false), Some(&kept(now - 20 * 3600)), clock(now));
        assert_eq!(t.line, "Kraken is busy · using yesterday's 19:00 price");
        // None at all, or one too old to use.
        let t = toast(&alert(Failure::NotAnswering, None, false), None, clock(now));
        assert_eq!(t.line, "No price yet · amounts stay in XMR");
        let t = toast(&alert(Failure::NotAnswering, None, true), None, clock(now));
        assert_eq!(t.line, "No price yet · type the amount in XMR");
        let t = toast(&alert(Failure::BadAnswer, None, false), Some(&kept(now - super::super::SHOWN)), clock(now));
        assert_eq!(t.line, "Kraken answered without a price · amounts stay in XMR");
        // A day's history.
        let t = toast(&alert(Failure::NotAnswering, Some(crate::price::day(now) - 2), false), Some(&kept(at_1402)), clock(now));
        assert_eq!((t.title.as_str(), t.line.as_str()), ("Couldn't get 29 Oct's USD price", "Kraken didn't answer · shown as not available"));
        assert_eq!(t.retrying_line, "Asking Kraken for 29 Oct's USD price");
        // A twelve-hour phone.
        let t = toast(&alert(Failure::NotAnswering, None, false), Some(&kept(at_1402)), Clock { hours_24: false, ..clock(now) });
        assert_eq!(t.line, "Kraken didn't answer · using the 2:02 pm price");
        assert_eq!(reason(Failure::TorNotYet, "CoinGecko"), "Tor isn't built yet");
    }

    #[test]
    fn a_price_says_its_age_once_it_isnt_just_asked() {
        let now = 1_793_458_800;
        assert_eq!(age(now - super::super::AGED + 1, clock(now)), None);
        assert_eq!(age(now - 58 * 60, clock(now)).as_deref(), Some("price from 14:02"));
        assert_eq!(age(now - 20 * 3600, clock(now)).as_deref(), Some("price from yesterday 19:00"));
        assert_eq!(age(now - 3 * 86_400, clock(now)).as_deref(), Some("price from 28 Oct"));
    }
}
