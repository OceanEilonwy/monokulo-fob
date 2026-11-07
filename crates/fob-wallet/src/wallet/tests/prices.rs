use super::*;

#[tokio::test]
async fn a_payment_seen_now_keeps_the_price_of_the_moment() {
    let wallet = Wallet::create(&dir("price-now"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let fake = FakePrices::new(166_060_000, vec![]);
    // Nothing to price yet: the current price only, no history.
    assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap());
    assert_eq!(fake.asked(), (1, 0));
    assert_eq!(wallet.worth_now(XMR / 2).await.as_deref(), Some("$83.03"));
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("in", t - 120, XMR / 2));
        state.sent.push(Sent { not_confirmed: false, released: false, tx: "out".into(), to: "86Ux".into(), amount: XMR / 10, fee: 1, change: 0, time: t, height: None, receipt: None, note: None, pieces: None, back: vec![], recipient_name: None, signed: None });
        assert!(wallet.record_prices(&mut state));
        wallet.save_state(&state).unwrap();
    }
    let activity = wallet.activity().await;
    assert_eq!(price_of(&activity, "in"), Some((166_060_000, false)));
    assert_eq!(price_of(&activity, "out"), Some((166_060_000, false)));
    // Kept in the wallet file, with who priced it.
    let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
    let record = again.activity().await.into_iter().find(|a| a.tx == "in").unwrap().price.unwrap();
    assert_eq!((record.currency.as_str(), record.provider.as_str(), record.at), ("USD", "Kraken", t));
    // Asked again only after 5 minutes (a minute when pulled).
    assert!(!wallet.refresh_prices_from(&fake, Asked::Background, t + 299).await.unwrap());
    assert!(wallet.refresh_prices_from(&fake, Asked::User, t + 60).await.unwrap());
    assert_eq!(fake.asked(), (2, 0));
}

#[tokio::test]
async fn older_payments_are_priced_from_the_daily_history() {
    let wallet = Wallet::create(&dir("price-history"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let today = price::day(t);
    let fake = FakePrices::new(170_000_000, vec![(today - 10, 150_000_000), (today - 1, 160_000_000), (today, 165_000_000)]);
    {
        let mut state = wallet.inner.state.lock().await;
        state.outputs.push(received("ten-days", t - 10 * DAY, XMR));
        state.outputs.push(received("too-old", t - 900 * DAY, XMR));
        // No price on hand: nothing recorded yet.
        assert!(!wallet.record_prices(&mut state));
    }
    assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap());
    assert_eq!(fake.asked(), (1, 1), "the whole window, once");
    let activity = wallet.activity().await;
    assert_eq!(price_of(&activity, "ten-days"), Some((150_000_000, true)));
    // Older than Kraken's window: not available, and not asked about.
    assert_eq!(price_of(&activity, "too-old"), None);
    // A payment found later is priced from what's cached, as a sync
    // step finds it, and the history isn't asked for again that day.
    let mut state = wallet.inner.state.lock().await;
    state.outputs.push(received("yesterday", t - DAY, XMR));
    assert!(wallet.record_prices(&mut state));
    drop(state);
    assert_eq!(price_of(&wallet.activity().await, "yesterday"), Some((160_000_000, true)));
    state = wallet.inner.state.lock().await;
    state.outputs.push(received("last-week", t - 7 * DAY, XMR));
    assert!(!wallet.record_prices(&mut state), "not in the history fetched");
    drop(state);
    wallet.refresh_prices_from(&fake, Asked::Background, t + 600).await.unwrap();
    assert_eq!(fake.asked(), (2, 1));
    assert_eq!(price_of(&wallet.activity().await, "last-week"), None);
    // The next day it is.
    wallet.refresh_prices_from(&fake, Asked::Background, t + DAY).await.unwrap();
    assert_eq!(fake.asked(), (3, 2));
}

#[tokio::test]
async fn with_xmr_only_nothing_is_asked_or_shown() {
    let wallet = Wallet::create(&dir("price-off"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let fake = FakePrices::new(166_000_000, vec![(price::day(t), 165_000_000)]);
    wallet.refresh_prices_from(&fake, Asked::Background, t).await.unwrap();
    assert_eq!(wallet.set_currency(None).await.unwrap(), None);
    wallet.inner.state.lock().await.outputs.push(received("while-off", t, XMR));
    assert!(!wallet.refresh_prices_from(&fake, Asked::User, t + DAY).await.unwrap());
    assert_eq!(fake.asked(), (1, 0), "no request with XMR only");
    assert_eq!(wallet.worth_now(XMR).await, None);
    assert!(!wallet.record_prices(&mut *wallet.inner.state.lock().await));
    assert!(wallet.activity().await.iter().all(|a| a.price.is_none()));
    // Dollars again: the payment seen meanwhile is priced from what's cached.
    wallet.set_currency(Some("USD")).await.unwrap();
    assert_eq!(price_of(&wallet.activity().await, "while-off"), Some((166_000_000, false)));
}

#[tokio::test]
async fn a_failed_request_leaves_the_payment_not_available() {
    let wallet = Wallet::create(&dir("price-fail"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    wallet.inner.state.lock().await.outputs.push(received("in", t - 3 * DAY, XMR));
    let fake = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
    assert!(wallet.refresh_prices_from(&fake, Asked::Background, t).await.is_err());
    assert_eq!(fake.asked(), (1, 1));
    assert_eq!(price_of(&wallet.activity().await, "in"), None);
    assert_eq!(wallet.worth_now(XMR).await, None);
}

#[tokio::test]
async fn failed_lookups_show_once_per_run_in_the_background_and_always_for_the_user() {
    let wallet = Wallet::create(&dir("price-alerts"), [1; 32], Network::Stagenet).unwrap();
    // Yesterday's noon, UTC: the steps below never cross midnight, so
    // the kept price is "the hh:mm price", not "yesterday's".
    let t = now() / DAY * DAY - DAY / 2;
    let clock = Clock { now: t, utc_offset: 0, hours_24: true };
    let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
    let working = FakePrices::new(166_100_000, vec![]);
    assert_eq!(wallet.price_from_line(clock).await, ("Kraken · web".into(), false));
    // The first background failure shows.
    assert!(wallet.refresh_prices_from(&failing, Asked::Background, t).await.is_err());
    let first = wallet.price_toast(clock).await.unwrap();
    assert_eq!((first.title.as_str(), first.line.as_str()), ("Couldn't get the USD price", "Kraken didn't answer · no USD price yet"));
    // Settings' row says the same, red.
    assert_eq!(wallet.price_from_line(clock).await, ("Kraken didn't answer · no USD price yet".into(), true));
    // Five minutes on, failing still: not again.
    wallet.refresh_prices_from(&failing, Asked::Background, t + 300).await.unwrap_err();
    assert_eq!(wallet.price_toast(clock).await.unwrap().id, first.id);
    // Pulled: shown again.
    wallet.refresh_prices_from(&failing, Asked::User, t + 400).await.unwrap_err();
    let pulled = wallet.price_toast(clock).await.unwrap();
    assert!(pulled.id > first.id);
    // A lookup that works ends it.
    wallet.refresh_prices_from(&working, Asked::Background, t + 600).await.unwrap();
    assert_eq!(wallet.price_toast(clock).await, None);
    assert!(!wallet.price_from_line(clock).await.1);
    // The next run's first background failure shows, with the price kept.
    wallet.refresh_prices_from(&failing, Asked::Background, t + 900).await.unwrap_err();
    let later = wallet.price_toast(Clock { now: t + 900, ..clock }).await.unwrap();
    assert!(later.id > pulled.id);
    assert!(later.line.starts_with("Kraken didn't answer · using the "), "{}", later.line);
    // Changing currency forgets the run (and asks at once, as the user).
    wallet.set_currency(Some("EUR")).await.unwrap();
    assert_eq!(wallet.price_toast(clock).await, None);
    assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), Some(Asked::User));
}

/// A lookup that comes back after a newer one has changes nothing: a
/// background failure landing after Retry worked doesn't bring the
/// toast back, and an older price doesn't overwrite a newer one.
#[tokio::test]
async fn an_older_lookup_coming_back_late_changes_nothing() {
    let wallet = Wallet::create(&dir("price-late"), [1; 32], Network::Stagenet).unwrap();
    let clock = clock_now();
    let late = |fail, price| {
        let hold = Arc::new(tokio::sync::Notify::new());
        let fake = Arc::new(FakePrices { fail, hold: Some(hold.clone()), ..FakePrices::new(price, vec![]) });
        let (w, f) = (wallet.clone(), fake.clone());
        let task = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::Background, clock.now).await });
        (fake, hold, task)
    };
    let asked = |fake: &FakePrices| fake.asked().0 == 1;
    // A background lookup hangs; Retry works meanwhile.
    let (fake, hold, background) = late(true, 1);
    while !asked(&fake) {
        tokio::task::yield_now().await;
    }
    let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, vec![]), clock).await;
    assert!(matches!(worked, Retried::Updated { .. }), "{worked:?}");
    hold.notify_one();
    assert!(!background.await.unwrap().unwrap(), "dropped");
    assert_eq!(wallet.price_toast(clock).await, None, "no toast after Retry worked");
    // An older lookup's price landing after a newer one's is dropped too.
    wallet.inner.state.lock().await.price_cache.current = None;
    let (fake, hold, background) = late(false, 150_000_000);
    while !asked(&fake) {
        tokio::task::yield_now().await;
    }
    wallet.refresh_prices_from(&FakePrices::new(170_000_000, vec![]), Asked::User, clock.now).await.unwrap();
    hold.notify_one();
    background.await.unwrap().unwrap();
    assert_eq!(wallet.price_now().await.map(|(_, p)| p), Some(170_000_000));
}

/// Swapping Home's balance while a lookup is out keeps what the lookup
/// brings: the order is only how amounts are drawn, not what's asked.
#[tokio::test]
async fn a_price_asked_while_the_balance_swaps_is_kept() {
    let wallet = Wallet::create(&dir("price-swap-while"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_currency(Some("USD")).await.unwrap();
    let t = now();
    let hold = Arc::new(tokio::sync::Notify::new());
    let fake = Arc::new(FakePrices { hold: Some(hold.clone()), ..FakePrices::new(166_100_000, vec![]) });
    let (w, f) = (wallet.clone(), fake.clone());
    let lookup = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::User, t).await });
    while fake.asked().0 == 0 {
        tokio::task::yield_now().await;
    }
    wallet.set_fiat_first(true).await.unwrap();
    hold.notify_one();
    assert!(lookup.await.unwrap().unwrap(), "kept");
    assert_eq!(wallet.balance_worth(XMR, t).await.0, Some(("$166.10".into(), None)));
    // Another currency meanwhile is another question: that answer goes.
    let (w, f) = (wallet.clone(), fake.clone());
    wallet.inner.state.lock().await.price_cache.current = None;
    let lookup = tokio::spawn(async move { w.refresh_prices_from(&*f, Asked::User, t).await });
    while fake.asked().0 == 1 {
        tokio::task::yield_now().await;
    }
    wallet.set_currency(Some("EUR")).await.unwrap();
    hold.notify_one();
    assert!(!lookup.await.unwrap().unwrap(), "dropped");
}

/// An older file's Tor stays Tor: the lookups fail saying why rather
/// than going over the web unannounced.
#[tokio::test]
async fn a_wallet_file_s_tor_is_never_moved_to_the_web_on_opening() {
    let path = dir("price-tor-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    file["state"]["prices"] = serde_json::json!({"currency": "USD", "provider": "kraken", "transport": "tor", "fiat_first": false});
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(old.price_settings().await.transport, price::Transport::Tor);
    let fake = FakePrices::new(166_100_000, vec![]);
    let failed = old.refresh_prices_from(&fake, Asked::Background, now()).await.unwrap_err();
    assert!(matches!(failed, FobError::Price(PriceError::Failed(price::Failure::NotOverTor))));
    assert_eq!(fake.asked(), (0, 0), "nothing asked");
    assert_eq!(old.price_toast(clock_now()).await.unwrap().line, "Kraken doesn't answer over Tor · no USD price yet");
}

/// Swapping Home's balance only changes how amounts are drawn: no
/// lookup, and a run of failures goes on.
#[tokio::test]
async fn swapping_the_balance_asks_for_nothing() {
    let wallet = Wallet::create(&dir("price-show-first"), [1; 32], Network::Stagenet).unwrap();
    let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
    wallet.refresh_prices_from(&failing, Asked::Background, now()).await.unwrap_err();
    let shown = wallet.price_alert().unwrap().id;
    wallet.set_fiat_first(true).await.unwrap();
    assert!(wallet.price_settings().await.fiat_first);
    assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), None);
    assert_eq!(wallet.price_alert().map(|a| a.id), Some(shown));
    wallet.set_currency(Some("EUR")).await.unwrap();
    assert_eq!(*wallet.inner.prices_asked.lock().unwrap(), Some(Asked::User));
}

#[tokio::test]
async fn a_day_s_history_that_fails_is_named() {
    let wallet = Wallet::create(&dir("price-alert-day"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    wallet.inner.state.lock().await.outputs.push(received("in", t - 2 * DAY, XMR));
    wallet.inner.state.lock().await.outputs.push(received("older", t - 5 * DAY, XMR));
    // The current price came; the history didn't.
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    wallet.inner.state.lock().await.price_cache.history.clear();
    let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
    wallet.refresh_prices_from(&failing, Asked::Background, t + 60).await.unwrap_err();
    assert_eq!(failing.asked(), (0, 1), "only the history was due");
    let clock = Clock { now: t + 60, utc_offset: 0, hours_24: true };
    let toast = wallet.price_toast(clock).await.unwrap();
    let (today, _) = clock::local(t + 60, clock);
    let date = clock::date(price::day(t - 2 * DAY), today);
    assert_eq!(toast.title, format!("Couldn't get {date}'s USD price"));
    assert_eq!(toast.line, "Kraken didn't answer · shown as not available");
    // Retry finds it: the day's price is named, not the current one.
    let days = vec![(price::day(t - 2 * DAY), 160_120_000), (price::day(t - 5 * DAY), 150_000_000)];
    let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, days), clock).await;
    assert_eq!(worked, Retried::Updated { title: format!("{date}'s USD price found"), line: "1 XMR = $160.12".into() });
}

#[tokio::test]
async fn retry_says_what_came_of_it() {
    let wallet = Wallet::create(&dir("price-retry"), [1; 32], Network::Stagenet).unwrap();
    let clock = clock_now();
    let failing = FakePrices { fail: true, ..FakePrices::new(1, vec![]) };
    wallet.refresh_prices_from(&failing, Asked::Background, clock.now).await.unwrap_err();
    let shown = wallet.price_toast(clock).await.unwrap();
    // Failing again: the toast again, as a new showing.
    let Retried::Failed(again) = wallet.retry_prices_from(&failing, clock).await else { panic!("it failed") };
    assert!(again.id > shown.id);
    assert_eq!((again.retrying_title.as_str(), again.retrying_line.as_str()), ("Trying again…", "Asking Kraken for the USD price"));
    assert_eq!(failing.asked().0, 2);
    // Working: the words for 2 s, and the run is over.
    let worked = wallet.retry_prices_from(&FakePrices::new(166_100_000, vec![]), clock).await;
    assert_eq!(worked, Retried::Updated { title: "USD price updated".into(), line: "1 XMR = $166.10".into() });
    assert_eq!(wallet.price_toast(clock).await, None);
}

#[tokio::test]
async fn an_old_price_keeps_amounts_with_its_age_and_one_over_a_day_turns_money_off() {
    let wallet = Wallet::create(&dir("price-age"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let at = |now| Clock { now, utc_offset: 0, hours_24: true };
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    // Just asked: the rate.
    assert_eq!(wallet.said_field("20", true, at(t)).await.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
    // A refresh missed: when it's from, money still typed.
    let f = wallet.said_field("20", true, at(t + price::AGED)).await;
    assert!(f.in_fiat && f.can_switch);
    let from = fob_words::prices::from_when(&crate::price_alert::age(t, at(t + price::AGED)).unwrap());
    assert_eq!(f.line, Some(format!("≈ 0.1204 XMR · {from}")));
    // Over a day: XMR only, saying why.
    let f = wallet.said_field("20", true, at(t + price::SHOWN)).await;
    assert!(!f.in_fiat && !f.can_switch);
    assert_eq!(f.line.as_deref(), Some("No USD price now · type the amount in XMR"));
}

/// Everything converted at the shown price says how old it is once it's
/// missed a refresh, up to the day it's shown for: Coins, Home's
/// not-ready sums, a payment's "Now", Split up's footer and Confirm.
#[tokio::test]
async fn every_amount_at_an_old_price_says_how_old() {
    let wallet = Wallet::create(&dir("price-age-everywhere"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let at = |now| Clock { now, utc_offset: 0, hours_24: true };
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.outputs.push(received("in", t - 3600, XMR));
        let mut waiting = received("new", t - 60, XMR / 2);
        waiting.height = 95;
        state.outputs.push(waiting);
    }
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    // Fresh: no age anywhere.
    assert_eq!(wallet.price_age(t).await, None);
    assert_eq!(wallet.said_coins(at(t)).await.ready_sum, "1 · $166.10");
    assert_eq!(wallet.said_not_ready(at(t)).await.unwrap().price_age, None);
    assert_eq!(wallet.said_payment("in", at(t)).await.unwrap().rate_now.as_deref(), Some("1 XMR = $166.10"));
    let fresh = wallet.said_split_view(&[1], at(t)).await.footer;
    assert!(fresh.starts_with("Network fee about ") && !fresh.contains('·'), "{fresh}");
    // A second short of a day: still shown, and how old it is said.
    let late = t + price::SHOWN - 1;
    assert_eq!(wallet.price_age(late).await, Some(fiat::PriceAge::Hours(23)));
    assert_eq!(wallet.said_coins(at(late)).await.ready_sum, "1 · $166.10 · 23 h old");
    let n = wallet.said_not_ready(at(late)).await.unwrap();
    assert_eq!(n.price_age.as_deref(), Some("23 h old"));
    assert!(n.all_ready_line.ends_with(" · price 23 h old"), "{}", n.all_ready_line);
    assert_eq!(wallet.said_payment("in", at(late)).await.unwrap().rate_now.as_deref(), Some("1 XMR = $166.10 · 23 h old"));
    let footer = wallet.said_split_view(&[1], at(late)).await.footer;
    assert!(footer.starts_with("Network fee about ") && footer.ends_with(" · 23 h old price"), "{footer}");
    // A day: no price, so no age.
    assert_eq!(wallet.price_age(t + price::SHOWN).await, None);
}

/// Receive's code and its field convert at the same price: a new price
/// changes both, and says it's new.
#[tokio::test]
async fn receive_s_code_and_field_agree_after_a_new_price() {
    let wallet = Wallet::create(&dir("receive-new-price"), [1; 32], Network::Stagenet).unwrap();
    let address = wallet.receive_address().await.unwrap();
    let t = now();
    wallet.refresh_prices_from(&FakePrices::new(100_000_000, vec![]), Asked::Background, t - 20 * 3600).await.unwrap();
    let old = wallet.price_at(t).await;
    assert_eq!(wallet.receive_code(&address, Some("20"), true, None).await.unwrap().detail.as_deref(), Some("0.2 XMR"));
    wallet.refresh_prices_from(&FakePrices::new(200_000_000, vec![]), Asked::Typing, t).await.unwrap();
    assert_ne!(wallet.price_at(t).await, old, "a new price is told apart");
    let field = wallet.said_field("20", true, clock_now()).await;
    let code = wallet.receive_code(&address, Some("20"), true, None).await.unwrap();
    assert_eq!(field.line.as_deref(), Some("≈ 0.1 XMR · 1 XMR = $200.00"));
    assert_eq!(code.detail.as_deref(), Some("0.1 XMR"));
    assert_eq!(code.uri, format!("monero:{address}?tx_amount=0.1"));
}

/// With no price at all the field is in XMR, and opening or typing in it
/// asks for one (once a minute), as money typed against an old one does.
#[tokio::test]
async fn with_no_price_the_field_asks_for_one() {
    let wallet = Wallet::create(&dir("price-none-asks"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let at = |now| Clock { now, utc_offset: 0, hours_24: true };
    let asked = || wallet.inner.prices_asked.lock().unwrap().take();
    let f = wallet.amount_field("", false, at(t)).await;
    assert!(!f.can_switch);
    assert_eq!(asked(), Some(Asked::Typing), "opening asks");
    wallet.amount_field("0.5", false, at(t + 30)).await;
    assert_eq!(asked(), None, "not again within the minute");
    wallet.amount_field("0.55", false, at(t + 60)).await;
    assert_eq!(asked(), Some(Asked::Typing));
    // XMR only: never.
    wallet.set_currency(None).await.unwrap();
    asked();
    wallet.amount_field("1", false, at(t + 600)).await;
    assert_eq!(asked(), None);
}

/// The blank field a form opens with, ready before it's drawn: in money
/// at the shown price, XMR without one, and never a lookup of its own.
#[tokio::test]
async fn the_blank_field_follows_the_price_and_asks_nothing() {
    let wallet = Wallet::create(&dir("price-blank"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let at = |now| Clock { now, utc_offset: 0, hours_24: true };
    let asked = || wallet.inner.prices_asked.lock().unwrap().take();
    let f = wallet.blank_amount_field(at(t)).await;
    assert!(!f.in_fiat && !f.can_switch);
    assert_eq!(asked(), None, "no price, still no lookup");
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    let f = wallet.blank_amount_field(at(t)).await;
    assert!(f.in_fiat && f.can_switch);
    assert_eq!(f, wallet.amount_field("", true, at(t)).await, "the same as the form's own");
    // Over a day old: XMR again.
    assert!(!wallet.blank_amount_field(at(t + price::SHOWN)).await.in_fiat);
    assert_eq!(asked(), None);
}

#[tokio::test]
async fn money_typed_against_an_old_price_asks_again_at_most_once_a_minute() {
    let wallet = Wallet::create(&dir("price-typed"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    let at = |now| Clock { now, utc_offset: 0, hours_24: true };
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, t).await.unwrap();
    let asked = || wallet.inner.prices_asked.lock().unwrap().take();
    // A fresh price, XMR typed, or nothing typed: no lookup.
    wallet.amount_field("20", true, at(t + 60)).await;
    wallet.amount_field("0.5", false, at(t + price::REFRESH)).await;
    wallet.amount_field(" ", true, at(t + price::REFRESH)).await;
    assert_eq!(asked(), None);
    // Money typed against a price 5 minutes old: asked, as typing.
    wallet.amount_field("20", true, at(t + price::REFRESH)).await;
    assert_eq!(asked(), Some(Asked::Typing));
    // The next key within the minute: not again.
    wallet.amount_field("205", true, at(t + price::REFRESH + 59)).await;
    assert_eq!(asked(), None);
    wallet.amount_field("2050", true, at(t + price::REFRESH + 60)).await;
    assert_eq!(asked(), Some(Asked::Typing));
}

#[tokio::test]
async fn receive_takes_an_amount_in_money_and_asks_in_xmr() {
    let wallet = Wallet::create(&dir("receive-fiat"), [1; 32], Network::Stagenet).unwrap();
    let address = wallet.receive_address().await.unwrap();
    // No price yet: XMR only.
    assert!(!wallet.amount_field("", true, clock_now()).await.can_switch);
    assert!(matches!(wallet.receive_code(&address, Some("20"), true, None).await, Err(FobError::Screen(ScreenError::NoPriceToConvert))));
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, now()).await.unwrap();
    let field = wallet.said_field("20", true, clock_now()).await;
    assert_eq!((field.sign.as_deref(), field.unit.as_str(), field.in_fiat), (Some("$"), "USD", true));
    // What the field says the code asks for is what it asks for.
    assert_eq!(field.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));

    let code = wallet.receive_code(&address, Some("20"), true, Some("Coffee beans")).await.unwrap();
    assert_eq!(code.uri, format!("monero:{address}?tx_amount=0.1204&tx_description=Coffee%20beans"));
    assert_eq!(code.headline.as_deref(), Some("$20.00"));
    assert_eq!(code.detail.as_deref(), Some("0.1204 XMR · Coffee beans"));
    // In XMR, as before.
    let code = wallet.receive_code(&address, Some("0.5"), false, None).await.unwrap();
    assert_eq!((code.headline.as_deref(), code.detail), (Some("0.5 XMR"), None));
    assert!(wallet.receive_code(&address, Some("20.123"), true, None).await.is_err());
    // Switching the field converts what's typed.
    assert_eq!(wallet.convert_amount("20", false).await, "0.1204");
    assert_eq!(wallet.convert_amount("0.1204", true).await, "20.00");
    assert_eq!(wallet.convert_amount("", true).await, "");
    // XMR only: no money at all.
    wallet.set_currency(None).await.unwrap();
    assert_eq!(wallet.amount_field("20", true, clock_now()).await.unit, "XMR");
    assert_eq!(wallet.convert_amount("20", false).await, "");
}

#[tokio::test]
async fn send_takes_an_amount_in_money_at_the_current_price() {
    let wallet = Wallet::create(&dir("send-fiat"), [1; 32], Network::Stagenet).unwrap();
    let address = wallet.receive_address().await.unwrap();
    // No price: XMR only.
    assert_eq!(wallet.typed_amount(" 0.5 ", false).await.unwrap(), (XMR / 2, None));
    assert!(matches!(wallet.typed_amount("20", true).await, Err(FobError::Screen(ScreenError::NoPriceToConvert))));
    assert!(matches!(wallet.prepare_typed(&address, "20", true, None, None).await, Err(FobError::Screen(ScreenError::NoPriceToConvert))));
    wallet.refresh_prices_from(&FakePrices::new(166_100_000, vec![]), Asked::Background, now()).await.unwrap();
    // The XMR the field's line showed for it.
    assert_eq!(wallet.said_field("20", true, clock_now()).await.line.as_deref(), Some("≈ 0.1204 XMR · 1 XMR = $166.10"));
    assert_eq!(wallet.typed_amount("20", true).await.unwrap(), (120_400_000_000, Some("$20.00".into())));
    assert!(matches!(wallet.typed_amount("20.123", true).await, Err(FobError::Monero(MoneroError::BadAmount))));
    // Converted before the address and amount are checked, as prepare checks them.
    assert!(matches!(wallet.prepare_typed("86Ux", "20", true, None, None).await, Err(FobError::Monero(MoneroError::BadAddress(_)))));
    assert!(matches!(wallet.prepare_typed(&address, "0", true, None, None).await, Err(FobError::Monero(MoneroError::BadAmount))));
    assert!(matches!(wallet.prepare_typed(&address, "abc", false, None, None).await, Err(FobError::Monero(MoneroError::BadAmount))));
}

#[tokio::test]
async fn price_settings_refuse_what_fob_cant_do() {
    let wallet = Wallet::create(&dir("price-settings"), [1; 32], Network::Stagenet).unwrap();
    assert!(matches!(wallet.set_currency(Some("XYZ")).await, Err(FobError::Price(_))));
    assert_eq!(
        wallet.set_price_transport(price::Transport::Tor).await.unwrap_err().to_string(),
        "prices over Tor aren't available yet"
    );
    assert_eq!(wallet.set_currency(Some("EUR")).await.unwrap(), None);
    wallet.set_fiat_first(true).await.unwrap();
    let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
    let settings = again.price_settings().await;
    assert_eq!((settings.code(), settings.fiat_first, settings.transport), (Some("EUR"), true, price::Transport::Web));
}

#[tokio::test]
async fn a_currency_kraken_lacks_moves_to_coingecko_and_old_records_stay() {
    let wallet = Wallet::create(&dir("price-move"), [1; 32], Network::Stagenet).unwrap();
    let t = now();
    wallet.inner.state.lock().await.outputs.push(received("in", t, XMR));
    wallet.refresh_prices_from(&FakePrices::new(166_000_000, vec![]), Asked::Background, t).await.unwrap();
    assert_eq!(price_of(&wallet.activity().await, "in"), Some((166_000_000, false)));
    let said = wallet.set_currency(Some("NZD")).await.unwrap();
    assert_eq!(said.map(fob_words::prices::moved).as_deref(), Some("Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"));
    // Kept, and the dollar price no longer shows in New Zealand dollars.
    let again = Wallet::open(&wallet.inner.path, [1; 32]).unwrap();
    let settings = again.price_settings().await;
    assert_eq!((settings.code(), settings.provider), (Some("NZD"), price::Provider::CoinGecko));
    assert_eq!(price_of(&again.activity().await, "in"), None);
    assert_eq!(again.worth_now(XMR).await, None);
    // Kraken can't be picked for it; back in dollars, the record is there.
    assert!(again.set_price_provider(price::Provider::Kraken).await.is_err());
    again.set_currency(Some("USD")).await.unwrap();
    assert_eq!(again.price_settings().await.provider, price::Provider::CoinGecko, "CoinGecko prices dollars too");
    assert_eq!(price_of(&again.activity().await, "in"), Some((166_000_000, false)));
    assert_eq!(again.set_price_provider(price::Provider::Kraken).await.unwrap(), None);
    // The list ticks what's chosen; with XMR only there's nothing to check.
    let list = again.currency_list(Some("NZD"), "").await;
    assert!(list.groups[2].rows.iter().any(|r| r.chosen && r.code.as_deref() == Some("USD")));
    again.set_currency(None).await.unwrap();
    assert!(again.check_prices().await.is_none());
    assert!(again.currency_list(None, "").await.groups[0].rows[0].chosen);
}

#[tokio::test]
async fn a_wallet_file_from_before_prices_opens_with_them_on() {
    let path = dir("price-old-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    let state = file["state"].as_object_mut().unwrap();
    for new in ["prices", "price_cache", "price_records"] {
        assert!(state.remove(new).is_some(), "{new} is saved");
    }
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(old.price_settings().await, price::Settings::default());
}

#[tokio::test]
async fn a_wallet_file_with_prices_off_opens_with_xmr_only() {
    let path = dir("price-off-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    // As files had them before currencies: a switch and a currency.
    file["state"]["prices"] = serde_json::json!({"on": false, "currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": true});
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    let settings = old.price_settings().await;
    assert_eq!((settings.code(), settings.fiat_first), (None, true));
    file["state"]["prices"] = serde_json::json!({"on": true, "currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": false});
    old_file(&path, &file);
    drop(old);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(old.price_settings().await.code(), Some("EUR"));
}

/// A wallet saved with "Show first" on its currency (before the row
/// went) keeps that order; with XMR as the default currency nothing
/// swaps, and the order waits for a currency again.
#[tokio::test]
async fn an_old_file_s_currency_first_stays_and_xmr_never_swaps() {
    let path = dir("price-order-file");
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    let mut file = serde_json::to_value(FileOut { keys: StoredKeys::from(&wallet.inner.keys), state: &*wallet.inner.state.lock().await }).unwrap();
    // Without fiat_first at all: XMR first.
    file["state"]["prices"] = serde_json::json!({"currency": "EUR", "provider": "kraken", "transport": "web"});
    old_file(&path, &file);
    assert!(!Wallet::open(&path, [1; 32]).unwrap().price_settings().await.fiat_first);
    file["state"]["prices"] = serde_json::json!({"currency": "EUR", "provider": "kraken", "transport": "web", "fiat_first": true});
    old_file(&path, &file);
    let old = Wallet::open(&path, [1; 32]).unwrap();
    let t = now();
    let order = |can_swap, fiat_first| price::BalanceOrder { can_swap, fiat_first };
    let balance = async |w: &Wallet| w.balance_worth(XMR, t).await;
    assert_eq!(balance(&old).await, (None, order(false, false)), "no price yet: nothing to swap to");
    old.refresh_prices_from(&FakePrices::new(166_000_000, vec![]), Asked::Background, t).await.unwrap();
    let worth = Some(("€166.00".to_string(), None));
    assert_eq!(balance(&old).await, (worth.clone(), order(true, true)), "a worth whenever it swaps");
    old.set_fiat_first(false).await.unwrap();
    assert_eq!(balance(&old).await, (worth, order(true, false)));
    old.set_fiat_first(true).await.unwrap();
    // XMR: no swap, and tapping is refused.
    old.set_currency(None).await.unwrap();
    assert_eq!(balance(&old).await, (None, order(false, false)));
    assert!(matches!(old.set_fiat_first(false).await, Err(FobError::Screen(ScreenError::NothingToSwap))));
    // A currency again: its order as it was.
    old.set_currency(Some("EUR")).await.unwrap();
    assert!(Wallet::open(&path, [1; 32]).unwrap().price_settings().await.fiat_first);
}
