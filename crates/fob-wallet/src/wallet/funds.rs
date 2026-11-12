//! What the wallet holds: balance, coins, activity and a payment's details.

use super::*;

impl Wallet {
    pub async fn balance(&self) -> Balance {
        let state = self.inner.state.lock().await;
        let device = self.inner.keys.device().is_some();
        let mut balance = Balance::default();
        for output in state.outputs.iter().filter(|o| o.coin.is_free()) {
            balance.total = balance.total.saturating_add(output.amount);
            if confirmations(&state, output.height) >= state.needed(output) {
                balance.unlocked = balance.unlocked.saturating_add(output.amount);
                if device && output.coin.key_image().is_none() {
                    balance.unsynced = balance.unsynced.saturating_add(output.amount);
                }
            }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none() && !s.released) {
            balance.total = balance.total.saturating_add(sent.change);
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            balance.total = balance.total.saturating_add(pooled.amount());
        }
        balance
    }

    /// Each coin the wallet owns, the same money [`balance`](Wallet::balance)
    /// counts: those ready to spend (its unlocked outputs), then those not
    /// ready yet (outputs short of their confirmations, change from payments
    /// not yet mined, and payments to this wallet still in the pool, one
    /// piece per transaction there), and the last block's time.
    pub(super) async fn coin_pieces(&self) -> (Vec<Piece>, Vec<Piece>, u64) {
        let state = self.inner.state.lock().await;
        // Where a payment's own outputs came from: a split, or change named
        // as Activity titles the payment.
        let source = |sent: &Sent| match &sent.receipt_view {
            _ if sent.pieces.is_some() => Source::Split,
            Some(receipt) => Source::Change(Some(receipt.merchant.clone())),
            None => Source::Change(Some(short_title(&sent.to))),
        };
        let (mut ready, mut waiting) = (vec![], vec![]);
        for output in state.outputs.iter().filter(|o| o.coin.is_free()) {
            let needed = state.needed(output);
            let confirmations = confirmations(&state, output.height);
            let source = match state.sent.iter().find(|s| s.tx == output.tx) {
                Some(sent) => source(sent),
                None if state.spends.iter().any(|s| s.tx == output.tx) => Source::Change(None),
                None => Source::Received,
            };
            let piece = Piece { amount: output.amount, confirmations, needed, source, time: output.time };
            if confirmations >= needed { ready.push(piece) } else { waiting.push(piece) }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none() && !s.released && s.change > 0) {
            let back = if sent.back.is_empty() { std::slice::from_ref(&sent.change) } else { sent.back.as_slice() };
            for &amount in back {
                waiting.push(Piece { amount, confirmations: 0, needed: UNLOCK, source: source(sent), time: sent.time });
            }
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            waiting.push(Piece { amount: pooled.amount(), confirmations: 0, needed: UNLOCK, source: Source::Received, time: pooled.seen });
        }
        (ready, waiting, state.last_block_time)
    }

    /// Every coin, ready or not, as the Coins screen shows it.
    pub async fn coins(&self, clock: Clock) -> Coins {
        let (ready, waiting, last_block_time) = self.coin_pieces().await;
        let price = self.price_now().await;
        let age = self.price_age(clock.now).await;
        let mut coins = not_ready::coins(ready, waiting, last_block_time, UNLOCK, price.as_ref().map(|(c, p)| (c, *p)), age, clock);
        // A view-only wallet can't split, whatever it holds.
        coins.can_split &= self.can_spend();
        coins
    }

    /// The money not ready yet as Home shows it, beside what's ready to
    /// spend, or `None` when all of it is.
    pub async fn not_ready(&self, clock: Clock) -> Option<NotReady> {
        // Both halves from one look at the wallet, so they always add up.
        let (ready, pieces, last_block_time) = self.coin_pieces().await;
        let coins: Vec<u64> = ready.iter().map(|p| p.amount).collect();
        let mut summary = not_ready::summarise(pieces, amount::total(coins.iter().copied()), last_block_time, clock)?;
        summary.segments = fob_screens::coin_bar::segments(&coins, &summary.pieces);
        if let Some((currency, p)) = self.price_now().await {
            for sum in &mut summary.sums {
                sum.fiat = Some(fiat::value(sum.atomic, p, &currency));
            }
            summary.ready_money = fiat::forms(fiat::to_minor(summary.ready, p, &currency), &currency);
            summary.incoming_money = fiat::forms(fiat::to_minor(summary.total, p, &currency), &currency);
            summary.price_age = self.price_age(clock.now).await;
        }
        Some(summary)
    }

    /// Every payment in and out, newest first, unconfirmed ones on top.
    /// Built once for what the wallet holds and given again, shared, until
    /// that changes (decision 412).
    pub async fn activity(&self) -> Arc<[Activity]> {
        let state = self.inner.state.lock().await;
        let pool = self.inner.pool.lock().expect("pool lock");
        let key = Key { state: state.generation(), pool: pool.generation() };
        let mut kept = self.inner.activity.lock().expect("activity lock");
        if let Some(built) = kept.as_ref().filter(|b| b.key == key) {
            #[cfg(test)]
            built.check(&state, &pool);
            return built.rows.clone();
        }
        let rows: Arc<[Activity]> = build(&state, &pool).into();
        *kept = Some(Built { key, rows: rows.clone() });
        rows
    }

    /// The payment `tx` as its own screen shows it, or `None` if there's no
    /// such payment.
    pub async fn payment(&self, tx: &str, clock: Clock) -> Option<activity::Detail> {
        let activity = self.activity().await;
        let row = activity.iter().find(|a| a.tx == tx)?;
        let settings = self.price_settings().await;
        let now = self.price_now().await;
        let mut detail = activity::detail(row, now.as_ref().map(|(c, p)| (c, *p)), settings.code().is_some(), settings.provider.name(), clock);
        // "Now" at an old price says how old.
        detail.rate_age = self.price_age(clock.now).await;
        Some(detail)
    }

    /// Whether payment `tx` is one the node gave no answer for when it was
    /// sent, and that Fob still offers it with each sync (it keeps the signed
    /// transaction until the node takes it or turns it down). Not once it's
    /// mined, released, or marked not confirmed, when it's no longer offered.
    pub async fn unanswered(&self, tx: &str) -> bool {
        let state = self.inner.state.lock().await;
        state.sent.iter().any(|s| s.tx == tx && s.signed.is_some() && s.height.is_none() && !s.released && !s.not_confirmed)
    }
}

/// What the activity was built from: the state and the pool as they were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    state: u64,
    pool: u64,
}

/// The activity as last built, and what from.
pub(super) struct Built {
    key: Key,
    rows: Arc<[Activity]>,
}

impl Built {
    /// In tests, every activity given from what was kept is the activity
    /// built afresh: a change the key missed would show here.
    #[cfg(test)]
    fn check(&self, state: &State, pool: &HashMap<String, Pooled>) {
        if !MEASURING.with(|m| m.get()) {
            assert!(*self.rows == *build(state, pool), "the activity kept is not what the wallet holds now");
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Set while a test measures how long activity takes, so the check
    /// above doesn't count.
    pub(super) static MEASURING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A row with only what every row starts with.
fn row(tx: &str, direction: Direction, amount: u64, height: Option<u64>, time: u64) -> Activity {
    Activity {
        not_confirmed: false,
        released: false,
        tx: tx.to_owned(),
        direction,
        amount,
        fee: None,
        to: None,
        height,
        time,
        confirmations: 0,
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

/// Every payment in and out, newest first, unconfirmed ones on top: from
/// what the wallet holds and the pool, in one pass. Each question a row
/// asks (the coins a payment spent, a transaction's own outputs, the
/// request, share or contact at an address) is a map made once here, not
/// a search of everything per row, and each receipt was read when its
/// payment was recorded.
pub(super) fn build(state: &State, pool: &HashMap<String, Pooled>) -> Vec<Activity> {
    // Each transaction's own outputs, and the outputs each spent or holds,
    // in the wallet's order.
    let mut own: HashMap<&str, Vec<&Owned>> = HashMap::new();
    let mut spent: HashMap<[u8; 32], Vec<&Owned>> = HashMap::new();
    for output in &state.outputs {
        own.entry(output.tx.as_str()).or_default().push(output);
        if let Some(tx) = output.coin.tx() {
            spent.entry(tx).or_default().push(output);
        }
    }
    let none: Vec<&Owned> = vec![];
    let own_of = |tx: &str| own.get(tx).unwrap_or(&none);
    let spent_by = |tx: &str| hash_bytes(tx).and_then(|hash| spent.get(&hash)).unwrap_or(&none);
    // The first of each, as a search from the start would find it.
    let mut sent_by: HashMap<&str, &Sent> = HashMap::new();
    for sent in &state.sent {
        sent_by.entry(sent.tx.as_str()).or_insert(sent);
    }
    let mut request_at: HashMap<u32, &requests::Request> = HashMap::new();
    for request in state.requests.all() {
        request_at.entry(request.subaddress).or_insert(request);
    }
    let mut share_at: HashMap<u32, &profile::Share> = HashMap::new();
    for share in state.profile.shares() {
        share_at.entry(share.subaddress).or_insert(share);
    }
    let mut contact_by_id: HashMap<u64, &Contact> = HashMap::new();
    let mut owner: HashMap<&str, &Contact> = HashMap::new();
    for contact in state.contacts.all() {
        contact_by_id.entry(contact.id).or_insert(contact);
        for address in contact.addresses() {
            owner.entry(address).or_insert(contact);
        }
    }
    let code = state.prices.code();

    let mut rows = vec![];
    let mut outgoing: HashSet<&str> = HashSet::new();
    for sent in &state.sent {
        outgoing.insert(&sent.tx);
        rows.push(Activity {
            not_confirmed: sent.not_confirmed,
            released: sent.released,
            // A split's pieces are still this wallet's: only the fee left.
            fee: Some(sent.fee),
            to: Some(sent.to.clone()),
            confirmations: sent.height.map(|h| confirmations(state, h)).unwrap_or(0),
            receipt: sent.receipt_view.clone(),
            ..row(&sent.tx, Direction::Out, if sent.pieces.is_some() { sent.fee } else { sent.amount }, sent.height, sent.time)
        });
    }
    // Spends found on the chain that this wallet didn't send (a restored
    // wallet): what left is the spent inputs less change and fee. A fee
    // of 0 is one a light wallet server didn't give: not known, so the
    // row claims none and the amount includes it.
    for spend in &state.spends {
        if !outgoing.insert(&spend.tx) {
            continue;
        }
        let spent = amount::total(spent_by(&spend.tx).iter().map(|o| o.amount));
        let change = amount::total(own_of(&spend.tx).iter().map(|o| o.amount));
        rows.push(Activity {
            fee: (spend.fee > 0).then_some(spend.fee),
            confirmations: confirmations(state, spend.height),
            ..row(&spend.tx, Direction::Out, spent.saturating_sub(change).saturating_sub(spend.fee), Some(spend.height), spend.time)
        });
    }
    // Payments in, one row per transaction, in the order their first
    // output was found.
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    for output in state.outputs.iter().filter(|o| !outgoing.contains(o.tx.as_str())) {
        let at = *incoming.entry(&output.tx).or_insert_with(|| {
            rows.push(Activity { confirmations: confirmations(state, output.height), ..row(&output.tx, Direction::In, 0, Some(output.height), output.time) });
            rows.len() - 1
        });
        let row = &mut rows[at];
        row.amount = row.amount.saturating_add(output.amount);
        add_part(&mut row.parts, output.subaddress, output.amount);
    }
    // Those still in the pool, in the order of their hashes so the same
    // pool is the same rows.
    let mut pooled: Vec<(&String, &Pooled)> = pool.iter().filter(|(tx, _)| !outgoing.contains(tx.as_str()) && !incoming.contains_key(tx.as_str())).collect();
    pooled.sort_by_key(|(tx, _)| *tx);
    for (tx, pooled) in pooled {
        rows.push(Activity { subaddress: pooled.parts.first().map(|p| p.0), parts: pooled.parts.clone(), ..row(tx, Direction::In, pooled.amount(), None, pooled.seen) });
    }
    for row in &mut rows {
        if let Some(code) = code {
            row.price = state.price_records.get(&row.tx).and_then(|r| r.iter().find(|r| r.currency == code)).cloned();
        }
        match row.direction {
            Direction::In => {
                let outputs = own_of(&row.tx);
                row.pieces = outputs.iter().map(|o| o.amount).collect();
                row.needed = outputs.iter().map(|o| state.needed(o)).max().unwrap_or(UNLOCK);
                row.subaddress = row.subaddress.or(outputs.first().map(|o| o.subaddress));
                // A request's address: its note, and who it was for.
                match row.subaddress.and_then(|s| request_at.get(&s)) {
                    Some(request) => {
                        row.note = request.note.clone();
                        let payer = request.payer(&row.tx, row.time).and_then(|id| contact_by_id.get(&id));
                        row.contact = payer.map(|c| c.name.clone());
                        row.contact_id = payer.map(|c| c.id);
                    }
                    None => row.note = row.subaddress.and_then(|s| state.notes.get(&s)).cloned(),
                }
                // A share's address: "Via your profile".
                row.shared = row.subaddress.and_then(|s| share_at.get(&s)).map(|s| s.created);
            }
            Direction::Out => {
                row.pieces = spent_by(&row.tx).iter().map(|o| o.amount).collect();
                let sent = sent_by.get(row.tx.as_str());
                row.note = sent.and_then(|s| s.note.clone());
                row.split = sent.and_then(|s| s.pieces);
                // Sent to one of its own addresses: more came back than the change, in a block
                // or still in the pool. What came back is still this wallet's; only the fee left.
                if let Some(sent) = sent.filter(|s| s.pieces.is_none()) {
                    let mined = amount::total(own_of(&row.tx).iter().map(|o| o.amount));
                    let pooled = pool.get(&row.tx).map_or(0, Pooled::amount);
                    if mined.max(pooled) > sent.change {
                        row.to_self = true;
                        row.amount = sent.fee;
                    }
                }
                if let Some(owner) = row.to.as_deref().and_then(|to| owner.get(to)).filter(|_| row.split.is_none()) {
                    row.contact = Some(owner.name.clone());
                    row.contact_id = Some(owner.id);
                }
            }
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse((r.height.is_none(), r.height, r.time)));
    rows
}
