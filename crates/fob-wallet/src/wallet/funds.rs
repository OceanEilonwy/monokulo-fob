//! What the wallet holds: balance, coins, activity and a payment's details.

use super::*;

impl Wallet {
    pub async fn balance(&self) -> Balance {
        let state = self.inner.state.lock().await;
        let mut balance = Balance::default();
        for output in state.outputs.iter().filter(|o| o.spent_in.is_none()) {
            balance.total += output.amount;
            if confirmations(&state, output.height) >= state.needed(output) {
                balance.unlocked += output.amount;
            }
        }
        for sent in state.sent.iter().filter(|s| s.height.is_none() && !s.released) {
            balance.total += sent.change;
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            balance.total += pooled.amount();
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
        let source = |sent: &Sent| match sent.receipt.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok()) {
            _ if sent.pieces.is_some() => Source::Split,
            Some(receipt) => Source::Change(Some(ReceiptView::from(&receipt).merchant)),
            None => Source::Change(Some(short_title(&sent.to))),
        };
        let (mut ready, mut waiting) = (vec![], vec![]);
        for output in state.outputs.iter().filter(|o| o.spent_in.is_none()) {
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
        let ready = ready.iter().map(|p| p.amount).sum();
        let mut summary = not_ready::summarise(pieces, ready, last_block_time, clock)?;
        if let Some((currency, p)) = self.price_now().await {
            for sum in &mut summary.sums {
                sum.fiat = Some(fiat::value(sum.atomic, p, &currency));
            }
            summary.price_age = self.price_age(clock.now).await;
        }
        Some(summary)
    }

    /// Every payment in and out, newest first, unconfirmed ones on top.
    pub async fn activity(&self) -> Vec<Activity> {
        let state = self.inner.state.lock().await;
        let mut rows = vec![];
        let mut outgoing: HashSet<&str> = HashSet::new();
        for sent in &state.sent {
            outgoing.insert(&sent.tx);
            let receipt = sent.receipt.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
            rows.push(Activity {
                not_confirmed: sent.not_confirmed,
                released: sent.released,
                tx: sent.tx.clone(),
                direction: Direction::Out,
                // A split's pieces are still this wallet's: only the fee left.
                amount: if sent.pieces.is_some() { sent.fee } else { sent.amount },
                fee: Some(sent.fee),
                to: Some(sent.to.clone()),
                height: sent.height,
                time: sent.time,
                confirmations: sent.height.map(|h| confirmations(&state, h)).unwrap_or(0),
                needed: UNLOCK,
                receipt: receipt.as_ref().map(ReceiptView::from),
                receipt_json: sent.receipt.clone(),
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
            });
        }
        // Spends found on the chain that this wallet didn't send (a restored
        // wallet): what left is the spent inputs less change and fee.
        for spend in &state.spends {
            if !outgoing.insert(&spend.tx) {
                continue;
            }
            let spent: u64 = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(&spend.tx)).map(|o| o.amount).sum();
            let change: u64 = state.outputs.iter().filter(|o| o.tx == spend.tx).map(|o| o.amount).sum();
            rows.push(Activity {
                not_confirmed: false,
                released: false,
                tx: spend.tx.clone(),
                direction: Direction::Out,
                amount: spent.saturating_sub(change).saturating_sub(spend.fee),
                fee: Some(spend.fee),
                to: None,
                height: Some(spend.height),
                time: spend.time,
                confirmations: confirmations(&state, spend.height),
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
            });
        }
        let mut incoming: HashMap<&str, Activity> = HashMap::new();
        for output in state.outputs.iter().filter(|o| !outgoing.contains(o.tx.as_str())) {
            let row = incoming.entry(&output.tx).or_insert_with(|| Activity {
                not_confirmed: false,
                released: false,
                tx: output.tx.clone(),
                direction: Direction::In,
                amount: 0,
                fee: None,
                to: None,
                height: Some(output.height),
                time: output.time,
                confirmations: confirmations(&state, output.height),
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
            });
            row.amount += output.amount;
            add_part(&mut row.parts, output.subaddress, output.amount);
        }
        rows.extend(incoming.into_values());
        for (tx, pooled) in self.inner.pool.lock().expect("pool lock").iter() {
            if !rows.iter().any(|r| r.tx == *tx) {
                rows.push(Activity {
                    not_confirmed: false,
                    released: false,
                    tx: tx.clone(),
                    direction: Direction::In,
                    amount: pooled.amount(),
                    fee: None,
                    to: None,
                    height: None,
                    time: pooled.seen,
                    confirmations: 0,
                    needed: UNLOCK,
                    receipt: None,
                    receipt_json: None,
                    price: None,
                    subaddress: pooled.parts.first().map(|p| p.0),
                    pieces: vec![],
                    note: None,
                    split: None,
                    to_self: false,
                    contact: None,
                    contact_id: None,
                    shared: None,
                    parts: pooled.parts.clone(),
                });
            }
        }
        for row in &mut rows {
            if let Some(code) = state.prices.code() {
                row.price = state.price_records.get(&row.tx).and_then(|r| r.iter().find(|r| r.currency == code)).cloned();
            }
            match row.direction {
                Direction::In => {
                    let outputs: Vec<&Owned> = state.outputs.iter().filter(|o| o.tx == row.tx).collect();
                    row.pieces = outputs.iter().map(|o| o.amount).collect();
                    row.needed = outputs.iter().map(|o| state.needed(o)).max().unwrap_or(UNLOCK);
                    row.subaddress = row.subaddress.or(outputs.first().map(|o| o.subaddress));
                    // A request's address: its note, and who it was for.
                    match row.subaddress.and_then(|s| state.requests.at(s)) {
                        Some(request) => {
                            row.note = request.note.clone();
                            row.contact_id = state.requests.payer(request.subaddress, &row.tx, row.time);
                            row.contact = row.contact_id.and_then(|id| state.contacts.get(id)).map(|c| c.name.clone());
                            row.contact_id = row.contact.as_ref().and(row.contact_id);
                        }
                        None => row.note = row.subaddress.and_then(|s| state.notes.get(&s)).cloned(),
                    }
                    // A share's address: "Via your profile".
                    row.shared = row.subaddress.and_then(|s| state.profile.at(s)).map(|s| s.created);
                }
                Direction::Out => {
                    row.pieces = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(row.tx.as_str())).map(|o| o.amount).collect();
                    let sent = state.sent.iter().find(|s| s.tx == row.tx);
                    row.note = sent.and_then(|s| s.note.clone());
                    row.split = sent.and_then(|s| s.pieces);
                    // Sent to one of its own addresses: more came back than the change, in a block
                    // or still in the pool. What came back is still this wallet's; only the fee left.
                    if let Some(sent) = sent.filter(|s| s.pieces.is_none()) {
                        let mined: u64 = state.outputs.iter().filter(|o| o.tx == row.tx).map(|o| o.amount).sum();
                        let pooled = self.inner.pool.lock().expect("pool lock").get(&row.tx).map_or(0, Pooled::amount);
                        if mined.max(pooled) > sent.change {
                            row.to_self = true;
                            row.amount = sent.fee;
                        }
                    }
                    if let Some(owner) = row.to.as_deref().and_then(|to| state.contacts.owner(to)).filter(|_| row.split.is_none()) {
                        row.contact = Some(owner.name.clone());
                        row.contact_id = Some(owner.id);
                    }
                }
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse((r.height.is_none(), r.height, r.time)));
        rows
    }

    /// The payment `tx` as its own screen shows it, or `None` if there's no
    /// such payment.
    pub async fn payment(&self, tx: &str, clock: Clock) -> Option<activity::Detail> {
        let row = self.activity().await.into_iter().find(|a| a.tx == tx)?;
        let settings = self.price_settings().await;
        let now = self.price_now().await;
        let mut detail = activity::detail(&row, now.as_ref().map(|(c, p)| (c, *p)), settings.code().is_some(), settings.provider.name(), clock);
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
