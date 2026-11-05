//! What XMR is worth: price settings, lookups and the amount field.

use super::*;

impl Wallet {
    /// How prices are shown.
    pub async fn price_settings(&self) -> price::Settings {
        self.inner.state.lock().await.prices.clone()
    }

    /// The failure in the current run of failed price lookups, if one is
    /// going on.
    pub fn price_alert(&self) -> Option<crate::price_alert::Alert> {
        self.inner.price_alerts.lock().expect("price lock").alert().cloned()
    }

    /// The price toast, while lookups are failing: what failed and what Fob
    /// uses instead. The app shows one whose `id` it hasn't shown yet.
    pub async fn price_toast(&self, clock: Clock) -> Option<crate::price_alert::Toast> {
        let alert = self.price_alert()?;
        let state = self.inner.state.lock().await;
        Some(crate::price_alert::toast(&alert, state.price_cache.last(&state.prices), clock))
    }

    /// Settings' Price from row: "Kraken · web", or while
    /// lookups fail the toast's line ("Kraken didn't answer · using the
    /// 14:02 price"), and whether they do (the row is red, with Try again).
    pub async fn price_from_line(&self, clock: Clock) -> (String, bool) {
        match self.price_toast(clock).await {
            Some(toast) => (toast.line, true),
            None => (self.price_settings().await.source_text(), false),
        }
    }

    /// Has the price task ask now, for `asked`.
    pub(super) fn ask_prices(&self, asked: crate::price_alert::Asked) {
        *self.inner.prices_asked.lock().expect("price lock") = Some(asked);
        self.inner.prices_wake.notify_one();
    }

    pub(super) async fn change_prices<T>(&self, change: impl FnOnce(&mut price::Settings) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut settings = state.prices.clone();
        let said = change(&mut settings)?;
        if settings != state.prices {
            // Which leads on Home only changes how amounts are drawn: nothing to ask.
            let asks = (settings.code(), settings.provider, settings.transport) != (state.prices.code(), state.prices.provider, state.prices.transport);
            state.prices = settings;
            self.record_prices(&mut state);
            self.save_state(&state)?;
            if asks {
                // The run was about settings gone; the user's new ones are asked now.
                self.inner.price_alerts.lock().expect("price lock").reset();
                self.ask_prices(crate::price_alert::Asked::User);
            }
        }
        Ok(said)
    }

    /// "Default currency" `code`, one of the 45, or XMR (`None`),
    /// when Fob never asks for a price. Moves to a provider that prices it
    /// when the chosen one doesn't, and returns the words saying so.
    pub async fn set_currency(&self, code: Option<&str>) -> Result<Option<String>> {
        self.change_prices(|s| s.choose_currency(code).map_err(Into::into)).await
    }

    /// Prices from `provider`, one that prices the currency. Moves to the
    /// web when it doesn't answer over Tor, and returns the words saying so.
    pub async fn set_price_provider(&self, provider: price::Provider) -> Result<Option<String>> {
        self.change_prices(|s| s.choose_provider(provider).map_err(Into::into)).await
    }

    /// How price requests travel. Tor isn't available yet, and is refused,
    /// as is a way the provider doesn't answer over.
    pub async fn set_price_transport(&self, transport: price::Transport) -> Result<()> {
        self.change_prices(|s| s.choose_transport(transport).map_err(Into::into)).await
    }

    /// Price from, as it opens: every provider that prices the chosen
    /// currency asked for XMR's price now, all at once, over the chosen
    /// transport; `None` with XMR only.
    pub async fn check_prices(&self) -> Option<Vec<price::Check>> {
        let settings = self.price_settings().await;
        let currency = fiat::currency(settings.code()?);
        let providers = price::providers_for(&currency.code);
        Some(price::check(providers, &currency, settings.transport, settings.provider.name(), price::CHECK_TIMEOUT).await)
    }

    /// The "Default currency" list, the chosen one ticked: `phone` is
    /// the phone's own currency code, `query` what's typed in the search.
    pub async fn currency_list(&self, phone: Option<&str>, query: &str) -> price::CurrencyList {
        price::currency_list(self.price_settings().await.code(), phone, query)
    }

    /// Tapping Home's balance: the default currency first, or XMR. Refused
    /// with XMR as the default currency, where nothing swaps.
    pub async fn set_fiat_first(&self, fiat_first: bool) -> Result<()> {
        self.change_prices(|s| {
            if s.code().is_none() {
                return Err(ScreenError::NothingToSwap.into());
            }
            s.fiat_first = fiat_first;
            Ok(())
        })
        .await
    }

    /// Home's balance of `atomic` XMR at `now` (seconds since the epoch),
    /// from one look at the price, so the two always agree: what it's worth
    /// with the price's age once it's missed a refresh (as
    /// [`worth_aged`](Wallet::worth_aged)), and whether tapping it swaps XMR
    /// and the default currency (one besides XMR, with that worth) and
    /// which leads.
    pub async fn balance_worth(&self, atomic: u64, now: u64) -> (Option<(String, Option<fiat::PriceAge>)>, price::BalanceOrder) {
        let state = self.inner.state.lock().await;
        let shown = state.price_cache.shown(&state.prices, now);
        let worth = shown.map(|c| (fiat::value(atomic, c.price, &fiat::currency(&c.currency)), crate::price_alert::old(c.at, now)));
        let order = price::balance_order(&state.prices, worth.is_some());
        (worth, order)
    }

    /// The current price shown now, when amounts are also shown in a
    /// currency and one was asked within the day: (currency, price).
    pub async fn price_now(&self) -> Option<(fiat::Currency, u64)> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now()).map(|c| (fiat::currency(&c.currency), c.price))
    }

    /// When the price shown at `now` was asked: new with every new price,
    /// so a screen converting money knows to convert again.
    pub async fn price_at(&self, now: u64) -> Option<u64> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now).map(|c| c.at)
    }

    /// How old the price shown at `now` is, once it's missed a refresh;
    /// `None` while it's fresh, or without one.
    pub async fn price_age(&self, now: u64) -> Option<fiat::PriceAge> {
        let state = self.inner.state.lock().await;
        state.price_cache.shown(&state.prices, now).and_then(|c| crate::price_alert::old(c.at, now))
    }

    /// [`price_age`](Wallet::price_age) now, by the phone's clock.
    pub async fn price_age_now(&self) -> Option<fiat::PriceAge> {
        self.price_age(now()).await
    }

    /// What `atomic` XMR is worth now: "$83.03"; `None` without a current price.
    pub async fn worth_now(&self, atomic: u64) -> Option<String> {
        self.price_now().await.map(|(currency, p)| fiat::value(atomic, p, &currency))
    }

    /// What `atomic` XMR is worth at `now` (seconds since the epoch), with
    /// the price's age once it's missed a refresh; `None` without a price
    /// from within the day.
    pub async fn worth_aged(&self, atomic: u64, now: u64) -> Option<(String, Option<fiat::PriceAge>)> {
        let state = self.inner.state.lock().await;
        let c = state.price_cache.shown(&state.prices, now)?;
        Some((fiat::value(atomic, c.price, &fiat::currency(&c.currency)), crate::price_alert::old(c.at, now)))
    }

    /// An amount field for `text`, typed in money (`in_fiat`) or XMR: in
    /// money only with a currency besides XMR and a price from within the
    /// day, saying when it's from once it's aged (`clock`, the phone's).
    /// Money typed against a price 5 minutes old or more has the price
    /// task ask (at most once a minute), a failure shown as the user's; so
    /// does the field with no price at all (opened or typed in, the field
    /// then in XMR), while a currency is chosen.
    pub async fn amount_field(&self, text: &str, in_fiat: bool, clock: Clock) -> price::AmountField {
        let (wanted, shown) = {
            let state = self.inner.state.lock().await;
            (state.prices.code().map(fiat::currency), state.price_cache.shown(&state.prices, clock.now).cloned())
        };
        let stale = shown.as_ref().is_some_and(|c| clock.now.saturating_sub(c.at) >= price::REFRESH);
        if wanted.is_some() && (shown.is_none() || stale && in_fiat && !text.trim().is_empty()) {
            let mut last = self.inner.typed_asked.lock().expect("price lock");
            if clock.now.saturating_sub(*last) >= price::PULL_REFRESH {
                *last = clock.now;
                self.ask_prices(crate::price_alert::Asked::Typing);
            }
        }
        Self::with_field_price(wanted, shown, clock, |price| price::amount_field(text, in_fiat, price))
    }

    /// The amount field as a form opens it, blank: in money while there's a
    /// price, XMR otherwise. It asks for nothing, so it can be ready before
    /// the form is drawn; the form's own [`amount_field`](Self::amount_field)
    /// then asks, as opening it does.
    pub async fn blank_amount_field(&self, clock: Clock) -> price::AmountField {
        let (wanted, shown) = {
            let state = self.inner.state.lock().await;
            (state.prices.code().map(fiat::currency), state.price_cache.shown(&state.prices, clock.now).cloned())
        };
        Self::with_field_price(wanted, shown, clock, price::blank_field)
    }

    /// `make` given the field's price: the currency `wanted` at the price
    /// `shown`, with its age by `clock` once it's aged.
    pub(super) fn with_field_price<T>(
        wanted: Option<fiat::Currency>,
        shown: Option<price::Current>,
        clock: Clock,
        make: impl FnOnce(price::FieldPrice) -> T,
    ) -> T {
        let age = shown.as_ref().and_then(|c| crate::price_alert::age(c.at, clock));
        let price = match (&wanted, &shown) {
            (Some(c), Some(now)) => price::FieldPrice::At(c, now.price, age.as_deref()),
            (Some(c), None) => price::FieldPrice::Missing(c),
            (None, _) => price::FieldPrice::None,
        };
        make(price)
    }

    /// An amount field's text as typed, the other way: money to XMR (`to_fiat`
    /// false) or XMR to money, for switching the field. Blank, or nothing to
    /// convert with, gives blank.
    pub async fn convert_amount(&self, text: &str, to_fiat: bool) -> String {
        let Some((currency, p)) = self.price_now().await else { return String::new() };
        if to_fiat {
            amount::parse(text).map(|a| fiat::plain(fiat::to_minor(a, p, &currency), &currency)).unwrap_or_default()
        } else {
            fiat::parse(text, &currency).ok().and_then(|m| fiat::to_atomic(m, p, &currency)).map(amount::format).unwrap_or_default()
        }
    }

    /// An amount field's text as XMR (atomic units): typed in XMR, or in
    /// money (`in_fiat`) converted at the current price, with the money as
    /// written ("$20.00"). Throws when it isn't an amount, or there's no
    /// price to convert with.
    pub async fn typed_amount(&self, text: &str, in_fiat: bool) -> Result<(u64, Option<String>)> {
        let (atomic, fiat) = self.typed(text, in_fiat).await?;
        Ok((atomic, fiat.map(|(text, _)| text)))
    }

    /// As [`typed_amount`](Wallet::typed_amount), the money with its
    /// currency's code.
    pub(super) async fn typed(&self, text: &str, in_fiat: bool) -> Result<(u64, Option<(String, String)>)> {
        let text = text.trim();
        if !in_fiat {
            return Ok((amount::parse(text)?, None));
        }
        let (currency, p) = self.price_now().await.ok_or(FobError::Screen(ScreenError::NoPriceToConvert))?;
        let minor = fiat::parse(text, &currency)?;
        Ok((fiat::to_atomic(minor, p, &currency).ok_or(FobError::Monero(MoneroError::BadAmount))?, Some((fiat::format(minor, &currency), currency.code))))
    }

    /// Builds a payment of an amount as Send's field took it: typed in
    /// XMR, or in money (`in_fiat`) converted at the current price, the
    /// conversion the field's line showed. Otherwise as
    /// [`prepare`](Wallet::prepare).
    pub async fn prepare_typed(&self, to: &str, amount_text: &str, in_fiat: bool, note: Option<String>, code: Option<&str>) -> Result<Prepared> {
        let (atomic, _) = self.typed_amount(amount_text, in_fiat).await?;
        self.prepare(to, atomic, note, code).await
    }

    /// [`preview`](Wallet::preview) of an amount as Send's field took it,
    /// converted as [`prepare_typed`](Wallet::prepare_typed) converts it.
    pub async fn preview_typed(&self, to: &str, amount_text: &str, in_fiat: bool, code: Option<&str>) -> Result<Preview> {
        let (atomic, _) = self.typed_amount(amount_text, in_fiat).await?;
        self.preview(to, atomic, code)
    }

    /// Under Send's sheet at `now`: "1.8592 XMR ready to spend · $308.70",
    /// the money only with a price from within the day, and its age once
    /// it's missed a refresh ("… · $308.70 · 2 h old").
    pub async fn ready_line(&self, now: u64) -> String {
        let ready = self.balance().await.unlocked;
        fob_words::prices::ready_line(&amount::format_short(ready), self.worth_aged(ready, now).await)
    }

    /// Asks the provider for what's due, unless XMR only: the current price
    /// every 5 minutes (a minute when the user asked), the daily history
    /// once a day while a payment lacks a price. Returns whether anything
    /// changed. A failure goes to the run of failures as `asked` says.
    pub async fn refresh_prices(&self, asked: crate::price_alert::Asked) -> Result<bool> {
        let provider = self.inner.state.lock().await.prices.provider;
        self.refresh_prices_from(&provider, asked, now()).await
    }

    pub(super) async fn refresh_prices_from(&self, source: &impl price::Source, asked: crate::price_alert::Asked, now: u64) -> Result<bool> {
        let pulled = asked != crate::price_alert::Asked::Background;
        let (settings, code, current_due, history_due, lacking_day) = {
            let state = self.inner.state.lock().await;
            let settings = state.prices.clone();
            let Some(code) = settings.currency.clone() else { return Ok(false) };
            let window = price::window(source, now);
            // The newest day a payment lacks a price for, to name it.
            let lacking_day = self
                .payment_times(&state)
                .into_iter()
                .filter(|(tx, time)| {
                    window.contains(&price::day(*time))
                        && !state.price_records.get(tx).is_some_and(|r| r.iter().any(|r| r.currency == code))
                })
                .map(|(_, time)| price::day(time))
                .max();
            let current_due = price::current_due(&state.price_cache, &settings, now, pulled);
            let history_due = lacking_day.is_some() && price::history_due(&state.price_cache, &settings, now);
            (settings, code, current_due, history_due, lacking_day)
        };
        if !current_due && !history_due {
            return Ok(false);
        }
        let failed = |failure, day| crate::price_alert::Failed { failure, provider: source.name().into(), currency: code.clone(), day };
        // Kept over a transport the provider doesn't answer over (an older
        // wallet file): never asked another way without the user's pick.
        let refused = match settings.transport {
            t if !source.answers_over(t) => Some(price::Failure::NotOverTor),
            t if !t.available() => Some(price::Failure::TorNotYet),
            _ => None,
        };
        if let Some(failure) = refused {
            self.inner.price_alerts.lock().expect("price lock").looked_up(Some(failed(failure, None)), asked);
            return Err(PriceError::Failed(failure).into());
        }
        let lookup = self.inner.price_lookups.fetch_add(1, Ordering::Relaxed) + 1;
        let current = if current_due { Some(source.current(&code, settings.transport).await) } else { None };
        let history = if history_due { Some(source.daily(&code, price::window(source, now), settings.transport).await) } else { None };

        let mut state = self.inner.state.lock().await;
        // Which leads on Home (fiat_first) only changes how amounts are drawn.
        let asked_for = |s: &price::Settings| (s.code().map(str::to_owned), s.provider, s.transport);
        if asked_for(&state.prices) != asked_for(&settings) {
            // Changed while asking: these answers are for settings gone.
            return Ok(false);
        }
        // A lookup started after this one has already come back (Retry
        // over a background one hanging): this one's answer, price or
        // failure, is older news and is dropped.
        let done = self.inner.price_looked_up.fetch_max(lookup, Ordering::Relaxed);
        if done > lookup {
            return Ok(false);
        }
        // A failure in a word: the current price's first, else the day's.
        let kind = |e: &PriceError| match e {
            PriceError::Failed(f) => *f,
            _ => price::Failure::NotAnswering,
        };
        let failure = match (&current, &history) {
            (Some(Err(e)), _) => Some(failed(kind(e), None)),
            (_, Some(Err(e))) => Some(failed(kind(e), lacking_day)),
            _ => None,
        };
        self.inner.price_alerts.lock().expect("price lock").looked_up(failure, asked);
        let mut changed = false;
        let mut error = None;
        match current {
            Some(Ok(p)) => {
                state.price_cache.current = Some(price::Current { currency: code.clone(), provider: source.name().into(), price: p, at: now });
                changed = true;
            }
            Some(Err(e)) => error = Some(e),
            None => {}
        }
        match history {
            Some(Ok(days)) => {
                price::store_history(&mut state.price_cache, &settings, days, now);
                changed = true;
            }
            Some(Err(e)) => error = error.or(Some(e)),
            None => {}
        }
        if changed {
            self.record_prices(&mut state);
            self.save_state(&state)?;
        }
        match error {
            Some(e) => Err(e.into()),
            None => Ok(changed),
        }
    }

    /// Asks for prices while Fob is on screen, until [`stop_prices`]: at
    /// once, then every 5 minutes, and at once again when a price setting
    /// changes. `listener` hears of each new price, so the app redraws.
    ///
    /// [`stop_prices`]: Wallet::stop_prices
    pub fn start_prices(&self, listener: Arc<dyn SyncListener>) {
        self.stop_prices();
        let wallet = self.clone();
        let handle = crate::runtime().spawn(async move {
            loop {
                let asked = wallet.inner.prices_asked.lock().expect("price lock").take().unwrap_or(crate::price_alert::Asked::Background);
                let before = wallet.price_alert();
                let changed = wallet.refresh_prices(asked).await.unwrap_or(false);
                if changed || wallet.price_alert() != before {
                    listener.on_status(wallet.status());
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(price::REFRESH)) => {}
                    _ = wallet.inner.prices_wake.notified() => {}
                }
            }
        });
        *self.inner.prices_task.lock().expect("price lock") = Some(handle);
    }

    pub fn stop_prices(&self) {
        if let Some(handle) = self.inner.prices_task.lock().expect("price lock").take() {
            handle.abort();
        }
    }

    /// Pull to refresh: new blocks and the pool as [`sync_now`](Wallet::sync_now),
    /// and the price if a minute has passed. Returns when both are done.
    pub async fn pull(&self) {
        let _ = tokio::join!(self.sync_now(), self.refresh_prices(crate::price_alert::Asked::User));
    }

    /// The toast's Retry: asks again as the user, and says what came of it.
    pub async fn retry_prices(&self, clock: Clock) -> Retried {
        let provider = self.inner.state.lock().await.prices.provider;
        self.retry_prices_from(&provider, clock).await
    }

    pub(super) async fn retry_prices_from(&self, source: &impl price::Source, clock: Clock) -> Retried {
        // A run about one day's price says that day when it's found.
        let day = self.price_alert().and_then(|a| a.failed.day);
        let _ = self.refresh_prices_from(source, crate::price_alert::Asked::User, clock.now).await;
        if let Some(toast) = self.price_toast(clock).await {
            return Retried::Failed(toast);
        }
        let state = self.inner.state.lock().await;
        let (title, line) = crate::price_alert::updated(&state.price_cache, &state.prices, day, clock);
        Retried::Updated { title, line }
    }
}
