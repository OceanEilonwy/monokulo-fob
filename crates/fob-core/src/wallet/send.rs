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
        self.inner.keys.spend()?;
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
        let destination = MoneroAddress::from_str(network.monero(), to.trim()).map_err(|_| FobError::BadAddress(network.name()))?;
        if amount == 0 {
            return Err(FobError::BadAmount);
        }
        Ok((destination, uri::code_terms(network, to, amount, code)))
    }

    /// Asks the node its fee rate, keeping it for Split up's estimate.
    pub(super) async fn fee_rate(&self, rpc: &Daemon) -> Result<FeeRate> {
        let fee_rate = rpc.fee_rate(FeePriority::Normal, MAX_FEE_PER_WEIGHT).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        // The mask has no getter; it's the second half of the serialization.
        let bytes = fee_rate.serialize();
        let mask = bytes.get(8..16).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).unwrap_or(1);
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
        if let Ok((_, rpc, decoys, _)) = self.connection().await {
            let _ = self.send_ready(&rpc, &decoys).await;
        }
    }

    /// Asks the node its fee rate now, so Split up's fee estimate is the
    /// node's, not Monero's usual. Quietly keeps the last one on failure.
    pub async fn refresh_fee_rate(&self) {
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
        let (_, rpc, decoys, _) = self.connection().await?;
        let (fee_rate, latest) = self.send_ready(&rpc, &decoys).await?;

        let (mut candidates, available) = {
            let state = self.inner.state.lock().await;
            let mut candidates: Vec<(usize, u64, WalletOutput)> = state
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, o)| {
                    o.spent_in.is_none()
                        && confirmations(&state, o.height) >= state.needed(o)
                })
                .filter_map(|(i, o)| {
                    let bytes = hex::decode(&o.output).ok()?;
                    Some((i, o.amount, WalletOutput::read(&mut bytes.as_slice()).ok()?))
                })
                .collect();
            candidates.sort_by_key(|c| std::cmp::Reverse(c.1));
            let available = candidates.iter().map(|c| c.1).sum::<u64>();
            (candidates, available)
        };
        let not_enough = |needed: u64| FobError::NotEnoughFunds { available: amount::format(available), needed: amount::format(needed) };
        if available < amount {
            return Err(not_enough(amount));
        }

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
        // Every input's ring at once: each is a round trip to the node.
        let mut rings = tokio::task::JoinSet::new();
        for (i, c) in chosen.iter().enumerate() {
            let (decoys, output) = (decoys.clone(), c.2.clone());
            rings.spawn(async move { (i, OutputWithDecoys::new(&mut OsRng, &decoys, RING_LEN, latest, output).await) });
        }
        let mut with_decoys: Vec<Option<OutputWithDecoys>> = vec![None; chosen.len()];
        while let Some(done) = rings.join_next().await {
            let (i, ring) = done.map_err(|e| FobError::Node(format!("{e}")))?;
            with_decoys[i] = Some(ring.map_err(|e| FobError::Node(format!("{e:?}")))?);
        }
        let mut with_decoys: Vec<OutputWithDecoys> = with_decoys.into_iter().flatten().collect();
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
                    with_decoys.push(OutputWithDecoys::new(&mut OsRng, &decoys, RING_LEN, latest, c.2.clone()).await.map_err(|e| FobError::Node(format!("{e:?}")))?);
                    chosen.push(c);
                }
                Err(e) => return Err(FobError::Send(format!("{e:?}"))),
            }
        };
        let fee = signable.necessary_fee();
        let receipt = receipt_json.as_deref().and_then(|j| serde_json::from_str::<orf_codec::Receipt>(j).ok());
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner.prepared.lock().await.insert(
            id,
            PreparedTx {
                signable,
                out: Outgoing {
                    to: to.to_owned(),
                    amount,
                    inputs: chosen.iter().map(|c| c.0).collect(),
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
        split::view(&notes, counts, &payments, &budget, money.as_ref().map(|(c, p)| (c, *p)), age.as_deref())
    }

    /// One more piece of note `index`, or why not (the 15-piece cap, or not
    /// enough ready to spend with the fee).
    pub async fn split_add(&self, counts: &[u32], index: usize) -> Result<Vec<u32>> {
        let (_, notes, budget) = self.split_setup().await;
        split::add(&notes, counts, index, &budget).map_err(FobError::Split)
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
        self.inner.keys.spend()?;
        let (_, notes, budget) = self.split_setup().await;
        let amounts = split::amounts(&notes, counts);
        if amounts.is_empty() {
            return Err(FobError::Split("Add a coin to split".into()));
        }
        split::check(&notes, counts, &budget).map_err(FobError::Split)?;
        let address = self.split_address().await?;
        let network = self.network();
        let destination = MoneroAddress::from_str(network.monero(), &address).map_err(|_| FobError::BadAddress(network.name()))?;
        let n = amounts.len() as u32;
        let payments = amounts.into_iter().map(|a| (destination, a)).collect();
        let mut prepared = self.prepare_payments(&address, payments, None, None, None, Some(n)).await?;
        prepared.split = split::lines(&notes, counts);
        Ok(prepared)
    }

    /// Signs and broadcasts a prepared payment. Returns its transaction hash.
    pub async fn send(&self, id: u64) -> Result<String> {
        let prepared = self.inner.prepared.lock().await.take(id).ok_or(FobError::NoSuchPayment)?;
        let spend = self.inner.keys.spend()?;
        let fee = prepared.signable.necessary_fee();
        let tx = prepared.signable.sign(&mut OsRng, spend).map_err(|e| FobError::Send(format!("{e:?}")))?;
        let hash = hex::encode(tx.hash());
        let wallet = self.clone();
        let manual_sync = self.inner.state.lock().await.manual_sync;
        let publish = async move {
            let rpc = wallet.rpc().await?;
            rpc.publish_transaction(&tx).await.map_err(|e| publish_failure(e, manual_sync))
        };
        self.publish_and_record(hash, fee, prepared.out, publish).await
    }

    /// Publishes a signed payment with `publish`, then records it (its
    /// inputs spent, the payment kept), as one step that runs to the end on
    /// its own once begun: dropping this call (the app leaving Confirm while
    /// "Sending…") never leaves a payment the node may have unrecorded.
    pub(super) async fn publish_and_record(&self, hash: String, fee: u64, out: Outgoing, publish: impl Future<Output = Result<()>> + Send + 'static) -> Result<String> {
        let wallet = self.clone();
        let step = tokio::spawn(async move {
            {
                // Spent already by a payment sent in the meantime?
                let state = wallet.inner.state.lock().await;
                if out.inputs.iter().any(|&i| state.outputs.get(i).is_none_or(|o| o.spent_in.is_some())) {
                    return Err(FobError::NoSuchPayment);
                }
            }
            publish.await?;
            wallet.record_sent(hash, fee, out).await
        });
        step.await.map_err(|e| FobError::Send(format!("{e}")))?
    }

    /// Keeps a payment just published: its inputs spent, and what only the
    /// sender knows.
    pub(super) async fn record_sent(&self, hash: String, fee: u64, out: Outgoing) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        for &i in &out.inputs {
            if let Some(output) = state.outputs.get_mut(i) {
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
        });
        self.record_prices(&mut state);
        self.save_state(&state)?;
        Ok(hash)
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
