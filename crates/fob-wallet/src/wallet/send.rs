//! Paying: fees, building a payment or a split, publishing and recording it.

use super::*;

impl Wallet {
    /// Builds a payment of `amount` to `to`, choosing the money to spend
    /// and the fee. Nothing leaves the wallet until [`send`](Wallet::send).
    /// `note` (typed, or the payment code's description) is kept with the
    /// payment once sent; `code` is the payment code the form was filled
    /// from, if any, whose name and receipt go with it as
    /// [`uri::code_terms`] says.
    pub async fn prepare(&self, to: &str, amount: u64, note: Option<String>, code: Option<&str>) -> Result<Prepared> {
        self.spendable()?;
        let (destination, terms) = self.payment_terms(to, amount, code)?;
        let receipt_json = terms.receipt.as_ref().and_then(crate::receipt::to_json);
        self.prepare_payments(to.trim(), vec![(destination, amount)], receipt_json, note, terms.name, None).await
    }

    /// What [`prepare`](Wallet::prepare) would pay, checked as it checks
    /// it, without asking the node: the confirm screen shows it at once
    /// while the fee is worked out.
    pub fn preview(&self, to: &str, amount: u64, code: Option<&str>) -> Result<Preview> {
        let (_, terms) = self.payment_terms(to, amount, code)?;
        Ok(Preview { to: to.trim().to_owned(), amount, receipt: terms.receipt.as_ref().map(ReceiptView::from) })
    }

    /// `to` as an address on this wallet's network, `amount` more than
    /// nothing, and what `code` gives the payment.
    pub(super) fn payment_terms(&self, to: &str, amount: u64, code: Option<&str>) -> Result<(MoneroAddress, uri::CodeTerms)> {
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), to.trim()).map_err(|_| FobError::Monero(MoneroError::BadAddress(network)))?;
        if amount == 0 {
            return Err(FobError::Monero(MoneroError::BadAmount));
        }
        Ok((destination, uri::code_terms(network, to, amount, code)))
    }

    /// Asks the node its fee rate, keeping it for Split up's estimate.
    /// Refused above [`MAX_FEE_PER_WEIGHT`], or rounded to more than
    /// [`MAX_FEE_MASK`].
    pub(super) async fn fee_rate(&self, rpc: &Daemon) -> Result<FeeRate> {
        let fee_rate = rpc.fee_rate(FeePriority::Normal, MAX_FEE_PER_WEIGHT).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        // The mask has no getter; it's the second half of the serialization.
        let bytes = fee_rate.serialize();
        let mask = bytes.get(8..16).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).unwrap_or(1);
        if mask > MAX_FEE_MASK {
            return Err(FobError::Node(format!("{:?}", monero_wallet::interface::FeeError::InvalidFee)));
        }
        *self.inner.fee_rate.lock().expect("fee lock") = Some((fee_rate.per_weight(), mask));
        Ok(fee_rate)
    }

    /// What a payment asks the node before it can be built: the fee rate and
    /// the chain tip, with the output distribution brought up to it. Asked
    /// together (each is a round trip), unless a moment ago, and asked once
    /// however many call it at once (see [`SendContext`]).
    pub(super) async fn send_ready(&self, rpc: &Daemon, decoys: &Decoys) -> Result<(FeeRate, usize)> {
        let node = self.node().await;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let (fee_rate, tip) = self
            .inner
            .send_context
            .get(&node, || async {
                let (fee_rate, tip) = tokio::join!(self.fee_rate(rpc), async {
                    let tip = rpc.latest_block_number().await.map_err(node_err)?;
                    // Brings the distribution up to the tip, so choosing decoys finds it ready.
                    decoys.update(tip).await.map_err(node_err)?;
                    Ok::<_, FobError>(tip)
                });
                Ok::<_, FobError>((fee_rate?, tip?))
            })
            .await?;
        // Choosing each input's decoys asks for the tip again; it is this one.
        decoys.note_tip(tip);
        // With nothing scanning, this is how coins waiting to unlock find out.
        {
            let mut state = self.inner.state.lock().await;
            let newest = tip as u64 + 1;
            if state.tip < newest {
                state.tip = newest;
                self.inner.unsaved.store(true, Ordering::Relaxed);
            }
        }
        Ok((fee_rate, tip))
    }

    /// Gets the node ready for a payment: opens the connection if it has
    /// gone quiet, and asks what building one needs (the fee rate and chain
    /// tip, the output distribution brought up to date), so that
    /// [`prepare`](Wallet::prepare) and the like, a moment later, wait for
    /// the node once instead of several times. Call it when the Send screen
    /// opens. It changes nothing, fails quietly, and is cheap to call again:
    /// what it fetched is kept for twenty seconds.
    pub async fn warm_up_send(&self) {
        // Through a server, only the fee is asked ahead.
        if self.light_mode().await {
            self.refresh_fee_rate().await;
            return;
        }
        if let Ok((_, rpc, decoys, _)) = self.connection().await {
            let _ = self.send_ready(&rpc, &decoys).await;
        }
    }

    /// Asks the node (or the light wallet server) its fee rate now, so Split
    /// up's fee estimate is its, not Monero's usual. Quietly keeps the last
    /// one on failure.
    pub async fn refresh_fee_rate(&self) {
        if self.light_mode().await {
            if let Ok(client) = self.light_client().await {
                let _ = self.light_fee(&client).await;
            }
            return;
        }
        if let Ok((_, rpc, _, _)) = self.connection().await {
            let _ = self.fee_rate(&rpc).await;
        }
    }

    /// Builds a transaction paying `payments`, choosing the money to spend
    /// and the fee. `to` is who the confirm screen says is paid.
    pub(super) async fn prepare_payments(
        &self,
        to: &str,
        payments: Vec<(MoneroAddress, u64)>,
        receipt_json: Option<String>,
        note: Option<String>,
        recipient_name: Option<String>,
        pieces: Option<u32>,
    ) -> Result<Prepared> {
        let amount: u64 = payments.iter().map(|p| p.1).sum();
        let device = self.inner.keys.device();
        let (mut candidates, available, unsynced) = {
            let state = self.inner.state.lock().await;
            let ready = |o: &Owned| o.spent_in.is_none() && confirmations(&state, o.height) >= state.needed(o);
            // A device's output can't be spent before the device has given
            // its key image (which says whether it's spent already).
            let synced = |o: &Owned| device.is_none() || (o.key_image.is_some() && o.tx_key.is_some());
            let unsynced = state.outputs.iter().filter(|o| ready(o) && !synced(o)).count();
            let mut candidates: Vec<((String, String), u64, WalletOutput)> = state
                .outputs
                .iter()
                .filter(|o| ready(o) && synced(o))
                .filter_map(|o| {
                    let bytes = hex::decode(&o.output).ok()?;
                    Some((o.id(), o.amount, WalletOutput::read(&mut bytes.as_slice()).ok()?))
                })
                .collect();
            candidates.sort_by_key(|c| std::cmp::Reverse(c.1));
            let available = candidates.iter().map(|c| c.1).sum::<u64>();
            (candidates, available, unsynced)
        };
        let not_enough = |needed: u64| {
            if unsynced > 0 {
                return FobError::TrezorNeeded;
            }
            FobError::Screen(ScreenError::NotEnoughFunds { available, needed })
        };
        if available < amount {
            return Err(not_enough(amount));
        }
        // The node's fee and decoys, or the light wallet server's.
        let (fee_rate, source) = self.send_source().await?;

        // Start with the inputs that cover the amount and a generous fee guess,
        // then add one at a time while the real fee needs more.
        let outputs = payments.len() + 1;
        let guess = |inputs: usize| fee_rate.calculate_fee_from_weight((1_600 + 900 * inputs as u64).max(split::weight(inputs, outputs))) as u64;
        let mut chosen = vec![];
        let mut total = 0;
        while total < amount + guess(chosen.len().max(1)) && !candidates.is_empty() {
            let c = candidates.remove(0);
            total += c.1;
            chosen.push(c);
        }
        // Every input's ring at once.
        let mut with_decoys = self.rings(&source, chosen.iter().map(|c| c.2.clone()).collect()).await?;
        let change = Change::new(self.inner.keys.view_pair(), None);
        let signable = loop {
            let mut outgoing_view = Zeroizing::new([0u8; 32]);
            OsRng.fill_bytes(outgoing_view.as_mut());
            match SignableTransaction::new(
                RctType::ClsagBulletproofPlus,
                outgoing_view,
                with_decoys.clone(),
                payments.clone(),
                change.clone(),
                vec![],
                fee_rate,
            ) {
                Ok(signable) => break signable,
                Err(SendError::NotEnoughFunds { necessary_fee, .. }) => {
                    if candidates.is_empty() {
                        return Err(not_enough(amount + necessary_fee.unwrap_or(0)));
                    }
                    let c = candidates.remove(0);
                    total += c.1;
                    with_decoys.extend(self.rings(&source, vec![c.2.clone()]).await?);
                    chosen.push(c);
                }
                Err(e) => return Err(FobError::Send(format!("{e:?}"))),
            }
        };
        let mut fee = signable.necessary_fee();
        let plan = match device {
            None => None,
            Some(Device::Trezor) => {
                // The device's transaction has the same shape as this one,
                // give or take a few bytes of extra: the fee for a little
                // more weight covers it, and is what the device shows.
                let weight = fee_rate.calculate_weight_from_fee(fee).unwrap_or(0) as u64 + trezor::sign::WEIGHT_MARGIN;
                fee = u64::try_from(fee_rate.calculate_fee_from_weight(weight)).map_err(|_| FobError::Send("the fee is too large".into()))?;
                if total < amount + fee {
                    return Err(not_enough(amount + fee));
                }
                let network = self.network();
                let change_address = MoneroAddress::from_str(network.monero(), &self.inner.keys.primary_address()).map_err(|_| FobError::Monero(MoneroError::BadAddress(network)))?;
                let state = self.inner.state.lock().await;
                let mut inputs = Vec::with_capacity(chosen.len());
                for (c, with_decoys) in chosen.iter().zip(with_decoys.iter()) {
                    let owned = state.outputs.iter().find(|o| o.id() == c.0).ok_or(FobError::Screen(ScreenError::NoSuchPayment))?;
                    let key = |hex_key: &Option<String>| hex_key.as_deref().and_then(|k| hex::decode(k).ok()).and_then(|k| <[u8; 32]>::try_from(k).ok());
                    inputs.push(trezor::sign::SpendInput {
                        with_decoys: with_decoys.clone(),
                        tx_pub_key: key(&owned.tx_key).ok_or(FobError::TrezorNeeded)?,
                        additional: key(&owned.extra_key),
                        index_in_tx: c.2.index_in_transaction(),
                        subaddress_minor: owned.subaddress,
                    });
                }
                Some(Plan { network, inputs, payments: payments.clone(), change: total - amount - fee, change_address, fee, hard_fork: HARD_FORK })
            }
        };
        let receipt = receipt_json.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner.prepared.lock().await.insert(
            id,
            PreparedTx {
                signable,
                plan,
                out: Outgoing {
                    to: to.to_owned(),
                    amount,
                    inputs: chosen.iter().map(|c| c.0.clone()).collect(),
                    input_total: total,
                    receipt: receipt.is_some().then_some(receipt_json).flatten(),
                    note: requests::clean_note(note.as_deref()),
                    recipient_name,
                    pieces,
                    back: if pieces.is_some() { payments.iter().map(|p| p.1).collect() } else { vec![] },
                },
            },
        );
        Ok(Prepared { id, to: to.to_owned(), amount, fee, receipt: receipt.as_ref().map(ReceiptView::from), split: vec![], pieces })
    }

    /// The notes Split up offers, in money at the current price when there
    /// is one, else in XMR; and the money the split can use.
    pub(super) async fn split_setup(&self) -> (Option<(fiat::Currency, u64)>, Vec<split::Note>, split::Budget) {
        let money = self.price_now().await;
        let notes = split::notes(money.as_ref().map(|(c, p)| (c, *p)));
        let state = self.inner.state.lock().await;
        let outputs = state.outputs.iter().filter(|o| o.spent_in.is_none() && confirmations(&state, o.height) >= state.needed(o)).map(|o| o.amount).collect();
        (money, notes, split::Budget { outputs, fee_rate: *self.inner.fee_rate.lock().expect("fee lock") })
    }

    /// What was paid in the 30 days to `now`, splits aside: what Split
    /// up's Auto learns from. The amounts Activity shows going out, read
    /// straight from what was sent and spent (no receipts parsed), as Split
    /// up asks again with every tap.
    pub(super) fn split_payments(state: &State, now: u64) -> Vec<u64> {
        let since = now.saturating_sub(split::LOOKBACK);
        let sent: HashSet<&str> = state.sent.iter().map(|s| s.tx.as_str()).collect();
        let mut spent: HashSet<&str> = HashSet::new();
        // A restored wallet's spends: the inputs less change and fee, as in Activity.
        let spends = state.spends.iter().filter(|s| !sent.contains(s.tx.as_str()) && spent.insert(&s.tx)).map(|spend| {
            let inputs: u64 = state.outputs.iter().filter(|o| o.spent_in.as_deref() == Some(&spend.tx)).map(|o| o.amount).sum();
            let change: u64 = state.outputs.iter().filter(|o| o.tx == spend.tx).map(|o| o.amount).sum();
            (spend.time, inputs.saturating_sub(change).saturating_sub(spend.fee))
        });
        state
            .sent
            .iter()
            .filter(|s| s.pieces.is_none())
            .map(|s| (s.time, s.amount))
            .chain(spends)
            .filter(|(time, amount)| *time >= since && *amount > 0)
            .map(|(_, amount)| amount)
            .collect()
    }

    /// Split up's screen for `counts` of each note, at `clock` (the
    /// phone's), which says how old the price is and which payments Auto
    /// learns from. The screen opens with no counts: empty.
    pub async fn split_view(&self, counts: &[u32], clock: Clock) -> split::View {
        let (money, notes, budget) = self.split_setup().await;
        let (payments, age) = {
            let state = self.inner.state.lock().await;
            (Self::split_payments(&state, clock.now), state.price_cache.shown(&state.prices, clock.now).and_then(|c| crate::price_alert::old(c.at, clock.now)))
        };
        split::view(&notes, counts, &payments, &budget, money.as_ref().map(|(c, p)| (c, *p)), age)
    }

    /// One more piece of note `index`, or why not (the 15-piece cap, or not
    /// enough ready to spend with the fee).
    pub async fn split_add(&self, counts: &[u32], index: usize) -> Result<Vec<u32>> {
        let (_, notes, budget) = self.split_setup().await;
        split::add(&notes, counts, index, &budget).map_err(FobError::Screen)
    }

    /// One fewer piece of note `index`.
    pub async fn split_remove(&self, counts: &[u32], index: usize) -> Vec<u32> {
        let (_, notes, _) = self.split_setup().await;
        split::remove(&notes, counts, index)
    }

    /// The address splits pay to: one of this wallet's subaddresses, the
    /// next unused one the first time, kept from then on and skipped by
    /// Receive.
    pub(super) async fn split_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        let index = match state.split_index {
            Some(index) => index,
            None => {
                let index = state.receive_index + 1;
                state.split_index = Some(index);
                self.save_state(&state)?;
                index
            }
        };
        Ok(self.inner.keys.address(index))
    }

    /// Builds a split: `counts` of each note paid to this wallet's split
    /// address, the rest back as change, for the confirm screen.
    pub async fn prepare_split(&self, counts: &[u32]) -> Result<Prepared> {
        self.spendable()?;
        let (_, notes, budget) = self.split_setup().await;
        let amounts = split::amounts(&notes, counts);
        if amounts.is_empty() {
            return Err(ScreenError::NoCoinToSplit.into());
        }
        split::check(&notes, counts, &budget).map_err(FobError::Screen)?;
        let address = self.split_address().await?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), &address).map_err(|_| FobError::Monero(MoneroError::BadAddress(network)))?;
        let n = amounts.len() as u32;
        let payments = amounts.into_iter().map(|a| (destination, a)).collect();
        let mut prepared = self.prepare_payments(&address, payments, None, None, None, Some(n)).await?;
        prepared.split = split::lines(&notes, counts);
        Ok(prepared)
    }

    /// Whether this wallet can pay at all: with its own spend key, or a
    /// device's.
    pub(super) fn spendable(&self) -> Result<()> {
        if self.inner.keys.can_spend() { Ok(()) } else { Err(FobError::Vault(VaultError::ViewOnly)) }
    }

    /// Signs and broadcasts a prepared payment. Returns its transaction hash.
    /// A wallet whose keys are on a device signs with
    /// [`send_with_trezor`](Wallet::send_with_trezor) instead.
    pub async fn send(&self, id: u64) -> Result<String> {
        if self.inner.keys.device().is_some() {
            return Err(FobError::TrezorNeeded);
        }
        let prepared = self.inner.prepared.lock().await.take(id).ok_or(FobError::Screen(ScreenError::NoSuchPayment))?;
        let spend = self.inner.keys.spend()?;
        let fee = prepared.signable.necessary_fee();
        let tx = prepared.signable.sign(&mut OsRng, &spend).map_err(|e| FobError::Send(format!("{e:?}")))?;
        let hash = hex::encode(tx.hash());
        let signed = hex::encode(tx.serialize());
        let publish = self.publishing(tx).await;
        self.publish_and_record(hash, fee, prepared.out, signed, publish).await
    }

    /// How a signed payment goes out, signed here or on a device: to the
    /// light wallet server syncing is through (`/submit_raw_tx`), or to the
    /// node. For [`publish_and_record`](Wallet::publish_and_record).
    pub(super) async fn publishing(&self, tx: monero_wallet::transaction::Transaction) -> impl Future<Output = std::result::Result<(), Unpublished>> + Send + 'static {
        let wallet = self.clone();
        let (manual_sync, light) = {
            let state = self.inner.state.lock().await;
            (state.manual_sync, state.sync_mode == SyncMode::Light)
        };
        async move {
            if light {
                return wallet.submit_through_server(tx.serialize()).await;
            }
            let rpc = wallet.rpc().await.map_err(Unpublished::Refused)?;
            rpc.publish_transaction(&tx).await.map_err(|e| publish_failure(e, manual_sync))
        }
    }

    /// Publishes a signed payment with `publish`, then records it (its
    /// inputs spent, the payment kept), as one step that runs to the end on
    /// its own once begun: dropping this call (the app leaving Confirm while
    /// "Sending…") never leaves a payment the node may have unrecorded.
    ///
    /// A node that gave no answer may have taken the payment and passed it
    /// on all the same, so it's recorded as sent, with `signed` (the signed
    /// transaction, in hex) kept to offer again with each sync. Recording
    /// nothing would leave its coins free, and paying again from other coins
    /// would pay twice. If it never got through, the day-old check marks it
    /// not confirmed, and its coins can be released.
    pub(super) async fn publish_and_record(
        &self,
        hash: String,
        fee: u64,
        out: Outgoing,
        signed: String,
        publish: impl Future<Output = std::result::Result<(), Unpublished>> + Send + 'static,
    ) -> Result<String> {
        let wallet = self.clone();
        let step = tokio::spawn(async move {
            // From the check to the record: another payment with one of these
            // coins, sent meanwhile, would pass the check too and both be sent.
            let _sending = wallet.inner.sending.lock().await;
            {
                // Spent already by a payment sent in the meantime, or gone
                // with a rescan since it was prepared?
                let state = wallet.inner.state.lock().await;
                if out.inputs.iter().any(|id| !state.outputs.iter().any(|o| o.is(id) && o.spent_in.is_none())) {
                    return Err(FobError::Screen(ScreenError::NoSuchPayment));
                }
            }
            let unanswered = match publish.await {
                Ok(()) => None,
                Err(Unpublished::Refused(e)) => return Err(e),
                Err(Unpublished::Unsure) => Some(signed),
            };
            wallet.record_sent(hash, fee, out, unanswered).await
        });
        step.await.map_err(|e| FobError::Send(format!("{e}")))?
    }

    /// Keeps a payment just published: its inputs spent, and what only the
    /// sender knows. `signed`, for a payment the node didn't answer for.
    pub(super) async fn record_sent(&self, hash: String, fee: u64, out: Outgoing, signed: Option<String>) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        for output in &mut state.outputs {
            if out.inputs.iter().any(|id| output.is(id)) {
                output.spent_in = Some(hash.clone());
            }
        }
        // A split's pieces come back to this wallet with the change.
        let back = if out.pieces.is_some() { out.input_total } else { out.input_total.saturating_sub(out.amount) };
        let change = out.input_total.saturating_sub(out.amount).saturating_sub(fee);
        let outputs = out.back.iter().copied().chain((change > 0).then_some(change)).collect();
        state.sent.push(Sent {
            not_confirmed: false,
            released: false,
            tx: hash.clone(),
            to: out.to,
            amount: out.amount,
            fee,
            change: back.saturating_sub(fee),
            time: now(),
            height: None,
            receipt: out.receipt,
            note: out.note,
            pieces: out.pieces,
            back: outputs,
            recipient_name: out.recipient_name,
            signed,
        });
        self.record_prices(&mut state);
        self.save_state(&state)?;
        // Seen in the pool before it was recorded: it's this wallet's own
        // payment, not money on its way in.
        self.inner.pool.lock().expect("pool lock").remove(&hash);
        Ok(hash)
    }

    /// Offers the node again each payment it gave no answer for, until it
    /// takes it or turns it down. Not once it's mined, released, or marked
    /// not confirmed: someone told it probably didn't go through may pay
    /// again by hand, and this one arriving late would pay twice.
    pub(super) async fn publish_unanswered(&self, rpc: &Daemon) {
        let waiting: Vec<(String, String)> = {
            let state = self.inner.state.lock().await;
            state.sent.iter().filter(|s| s.height.is_none() && !s.released && !s.not_confirmed).filter_map(|s| Some((s.tx.clone(), s.signed.clone()?))).collect()
        };
        let mut answered = vec![];
        for (tx, signed) in waiting {
            let read = hex::decode(&signed).ok().and_then(|bytes| monero_wallet::transaction::Transaction::read(&mut bytes.as_slice()).ok());
            let Some(transaction) = read else {
                answered.push(tx);
                continue;
            };
            match rpc.publish_transaction(&transaction).await {
                // Taken, or in its pool already.
                Ok(()) => answered.push(tx),
                // Turned down: mined already, its coins spent on the chain, or
                // never valid. Scanning, or the day-old check, says which.
                Err(monero_wallet::interface::PublishTransactionError::TransactionRejected(reason)) if turned_down(&reason) => answered.push(tx),
                // Still no answer, or a node too busy to look: next time.
                Err(_) => {}
            }
        }
        if answered.is_empty() {
            return;
        }
        let mut state = self.inner.state.lock().await;
        for sent in state.sent.iter_mut().filter(|s| answered.contains(&s.tx)) {
            sent.signed = None;
        }
        let _ = self.save_state(&state);
    }

    /// Forgets a prepared payment that won't be sent.
    pub async fn cancel(&self, id: u64) {
        self.inner.prepared.lock().await.take(id);
    }

    /// Every payment and when it moved: sent, spent on the chain, received,
    /// and on its way in the pool.
    pub(super) fn payment_times(&self, state: &State) -> Vec<(String, u64)> {
        let mut times: HashMap<String, u64> = HashMap::new();
        let sent = state.sent.iter().map(|s| (&s.tx, s.time));
        let spent = state.spends.iter().map(|s| (&s.tx, s.time));
        let received = state.outputs.iter().map(|o| (&o.tx, o.time));
        for (tx, time) in sent.chain(spent).chain(received) {
            times.entry(tx.clone()).or_insert(time);
        }
        for (tx, Pooled { seen, .. }) in self.inner.pool.lock().expect("pool lock").iter() {
            times.entry(tx.clone()).or_insert(*seen);
        }
        times.into_iter().collect()
    }

    /// Records what XMR was worth with each payment that has no price yet
    /// in the currency shown: the current price if it was asked within an
    /// hour of the payment, else the day's from the history. Nothing with
    /// XMR only; a payment with nothing to go on stays without, and gets
    /// one if the history later has its day. Returns whether any was
    /// recorded.
    pub(super) fn record_prices(&self, state: &mut State) -> bool {
        let Some(code) = state.prices.currency.clone() else { return false };
        let mut recorded = false;
        for (tx, time) in self.payment_times(state) {
            let records = state.price_records.get(&tx);
            if records.is_some_and(|r| r.iter().any(|r| r.currency == code)) {
                continue;
            }
            if let Some(record) = state.price_cache.value_at(&state.prices, time) {
                state.price_records.entry(tx).or_default().push(record);
                recorded = true;
            }
        }
        recorded
    }
}

/// Whether a node that didn't take a payment offered again, giving `reason`,
/// looked at it and turned it down. monero-daemon-rpc gives only the reason
/// for any status but OK, so a node still syncing, which answers BUSY without
/// looking, reads as a rejection with no reason. A real rejection names why
/// (double spend, fee too low, …); one that doesn't, or says the node is
/// busy, is offered again next time like no answer at all.
pub(super) fn turned_down(reason: &str) -> bool {
    let reason = reason.trim();
    !reason.is_empty() && !reason.to_ascii_lowercase().contains("busy")
}
