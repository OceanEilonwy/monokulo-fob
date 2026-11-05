//! Manual Sync, and payments gone quiet: scanning only when asked, pulls that
//! look up sent payments, and releasing what never arrived.

use super::*;

impl Wallet {
    /// With Manual Sync on and nothing asked for: the sync loop waits.
    pub(super) async fn waiting_to_be_asked(&self) -> bool {
        self.inner.state.lock().await.manual_sync && !self.inner.scan_asked.load(Ordering::Relaxed)
    }

    /// Tells the sync loop to scan to the newest block, as `title`.
    pub(super) async fn ask_to_scan(&self, title: &str, from: u64) {
        let (to, at) = {
            let state = self.inner.state.lock().await;
            (state.tip, state.next_block)
        };
        *self.inner.scan_job.lock().expect("scan lock") = Some(ScanProgress { title: title.into(), from, to, at, marks: vec![] });
        self.inner.scan_asked.store(true, Ordering::Relaxed);
        self.set_status(|s| {
            s.chain_changed = false;
            s.error = None;
            // Asks the node where the chain is first, however it was left.
            s.connected = false;
        });
        self.refresh_status().await;
        self.tell(self.status());
        self.inner.wake.notify_one();
    }

    /// The scan someone asked for is over.
    pub(super) async fn scan_finished(&self) {
        self.inner.scan_asked.store(false, Ordering::Relaxed);
        self.inner.scan_job.lock().expect("scan lock").take();
        self.refresh_status().await;
    }

    pub(super) fn scan_reached(&self, at: u64) {
        if let Some(job) = self.inner.scan_job.lock().expect("scan lock").as_mut() {
            job.at = at;
            job.to = job.to.max(at);
        }
    }

    pub(super) fn scan_mark(&self, height: u64, incoming: bool) {
        if let Some(job) = self.inner.scan_job.lock().expect("scan lock").as_mut() {
            match job.marks.iter_mut().find(|(h, _)| *h == height) {
                Some(mark) => mark.1 |= incoming,
                None => job.marks.push((height, incoming)),
            }
        }
    }

    /// This wallet's Manual Sync settings.
    pub async fn manual_settings(&self) -> ManualSettings {
        let state = self.inner.state.lock().await;
        state.manual()
    }

    /// Turns Manual Sync on or off. On, what's scanning stops after this batch,
    /// and instant updates stop too; off, syncing carries on from the last scan.
    pub async fn set_manual_sync(&self, on: bool) -> Result<()> {
        {
            let mut state = self.inner.state.lock().await;
            state.manual_sync = on;
            self.save_state(&state)?;
        }
        if on {
            self.inner.scan_asked.store(false, Ordering::Relaxed);
            self.inner.scan_job.lock().expect("scan lock").take();
        }
        self.restart_zmq().await;
        self.refresh_status().await;
        self.tell(self.status());
        self.inner.wake.notify_one();
        Ok(())
    }

    pub async fn set_pull_mode(&self, mode: PullMode) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        state.pull_mode = mode;
        self.save_state(&state)
    }

    /// How far back Auto's pull looks: 1 to [`manual::INCOMING_MAX`] blocks.
    pub async fn set_incoming_scan_blocks(&self, blocks: u32) -> Result<()> {
        if !(1..=manual::INCOMING_MAX).contains(&blocks) {
            return Err(FobError::Monero(MoneroError::BadAmount));
        }
        let mut state = self.inner.state.lock().await;
        state.incoming_scan_blocks = blocks;
        self.save_state(&state)
    }

    /// When a coin can be spent, and a payment is no longer checked: within
    /// [`manual::SETTLED_MIN`] and [`manual::SETTLED_MAX`] confirmations.
    pub async fn set_settled_confirmations(&self, confirmations: u32) -> Result<()> {
        if !(manual::SETTLED_MIN..=manual::SETTLED_MAX).contains(&confirmations) {
            return Err(FobError::Monero(MoneroError::BadAmount));
        }
        let mut state = self.inner.state.lock().await;
        state.settled_confirmations = confirmations;
        self.save_state(&state)
    }

    /// Payments this wallet sent that aren't settled yet.
    pub(super) fn unsettled_sent(state: &State) -> Vec<(String, Option<u64>)> {
        let settled = u64::from(state.settled_confirmations);
        state.sent.iter().filter(|s| !s.released && s.height.is_none_or(|h| confirmations(state, h) < settled)).map(|s| (s.tx.clone(), s.height)).collect()
    }

    /// Asks the node where the chain has got to, quietly keeping what's known on failure.
    pub async fn refresh_tip(&self) {
        if let Ok(rpc) = self.rpc().await
            && let Ok(latest) = rpc.latest_block_number().await
        {
            let mut state = self.inner.state.lock().await;
            state.tip = state.tip.max(latest as u64 + 1);
            self.inner.unsaved.store(true, Ordering::Relaxed);
        }
    }

    /// The Scan drawer's quick choices.
    pub async fn scan_options(&self) -> Vec<ScanOption> {
        let state = self.inner.state.lock().await;
        let first = state.restore_height.unwrap_or(0);
        manual::scan_options(state.manual(), Self::unsettled_sent(&state).len(), state.next_block, first, state.tip)
    }

    /// "Last scan Today, 14:20 · Block 3,811,200", or that there hasn't been one.
    pub async fn last_scan_line(&self, clock: Clock) -> String {
        let state = self.inner.state.lock().await;
        if state.next_block == 0 || state.last_block_time == 0 {
            return "No scan yet".into();
        }
        format!("Last scan {} · Block {}", catch_up::date_time(state.last_block_time, clock), manual::grouped(state.next_block - 1))
    }

    /// What the Block tab shows for a typed block.
    pub async fn block_info(&self, height: u64, clock: Clock) -> manual::BlockInfo {
        let state = self.inner.state.lock().await;
        let network = self.network();
        manual::BlockInfo {
            mined: network.dated(height).map(|time| format!("~{}", catch_up::date_time(time, clock))),
            newest: manual::grouped(state.tip.saturating_sub(1)),
            first: manual::grouped(state.restore_height.unwrap_or(0)),
        }
    }

    /// Starts what the Scan drawer was asked for, or a pull's words once
    /// [`ScanRequest::Unsettled`] is done. The scan itself carries on after
    /// this returns, in [`status`](Wallet::status)'s `scan`.
    pub async fn start_scan(&self, request: ScanRequest) -> Result<Option<PullOutcome>> {
        let (next_block, tip, first, newest_hash) = {
            let state = self.inner.state.lock().await;
            (state.next_block, state.tip, state.restore_height.unwrap_or(0), state.recent.last().cloned())
        };
        let (from, to, title) = match request {
            ScanRequest::Unsettled => {
                let mode = self.inner.state.lock().await.pull_mode;
                // Asked for by name: Disabled is about the gesture, not this.
                let mode = if mode == PullMode::Disabled { PullMode::Send } else { mode };
                return self.check_payments(mode).await.map(Some);
            }
            ScanRequest::FromLast => {
                // What pulls found past Last scan is put right as this passes it.
                *self.inner.pipeline.lock().await = None;
                self.rewind(next_block, newest_hash).await?;
                self.ask_to_scan("Scanning from last scan", next_block).await;
                return Ok(None);
            }
            ScanRequest::Whole => (first, u64::MAX, "Scanning the whole wallet".to_owned()),
            ScanRequest::Back(blocks) => (tip.saturating_sub(blocks).max(first), u64::MAX, format!("Scanning the newest {} blocks", manual::grouped(blocks))),
            ScanRequest::Height(height) => (height, u64::MAX, format!("Scanning from block {}", manual::grouped(height))),
            ScanRequest::Dates { from, to } => {
                let network = self.network();
                // Dates only say roughly which block: a margin, so none is missed.
                let start = network.restore_height_for(from);
                let end = to.map_or(u64::MAX, |to| network.height_at(to) + manual::BLOCKS_A_DAY);
                (start, end, "Scanning those dates".to_owned())
            }
        };
        match manual::scan_plan(from, to, next_block, tip) {
            ScanPlan::Rewind(from) => {
                *self.inner.pipeline.lock().await = None;
                self.rewind(from, None).await?;
                self.inner.state.lock().await.lower_start(from);
                self.ask_to_scan(&title, from).await;
            }
            ScanPlan::Window(from, to) => {
                *self.inner.scan_job.lock().expect("scan lock") = Some(ScanProgress { title, from, to: to + 1, at: from, marks: vec![] });
                self.inner.scan_stop.store(false, Ordering::Relaxed);
                self.refresh_status().await;
                self.tell(self.status());
                let wallet = self.clone();
                crate::runtime().spawn(async move {
                    let result = wallet.scan_blocks(from, to).await;
                    wallet.inner.scan_job.lock().expect("scan lock").take();
                    wallet.refresh_status().await;
                    if let Err(e) = result {
                        wallet.set_status(|s| s.error = Some(e.to_string()));
                    }
                    wallet.tell(wallet.status());
                });
            }
        }
        Ok(None)
    }

    /// Stops the scan someone asked for. What it found stays.
    pub async fn stop_scan(&self) {
        self.inner.scan_stop.store(true, Ordering::Relaxed);
        self.scan_finished().await;
        self.flush().await;
        self.tell(self.status());
    }

    /// Says something only once.
    pub fn clear_notice(&self) {
        self.set_status(|s| s.notice = None);
    }

    /// Pulling down on Home with Manual Sync on: what [`PullMode`] says. `None`
    /// when it says nothing.
    pub async fn pull_refresh(&self) -> Result<Option<PullOutcome>> {
        let mode = self.inner.state.lock().await.pull_mode;
        if mode == PullMode::Disabled {
            return Ok(None);
        }
        let (checked, _) = tokio::join!(self.check_payments(mode), self.refresh_prices(crate::price_alert::Asked::User));
        checked.map(Some)
    }

    /// Finds out where the chain is and whether it changed, updates the payments
    /// this wallet sent, and with Auto looks for new money in the pool and the
    /// newest blocks. Received payments aren't looked up by name: that would tell
    /// the node which ones are this wallet's. Last scan stays where it is.
    pub(super) async fn check_payments(&self, mode: PullMode) -> Result<PullOutcome> {
        self.inner.scan_stop.store(false, Ordering::Relaxed);
        let (_, rpc, _, _) = self.connection().await?;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let newest = rpc.latest_block_number().await.map_err(node_err)? as u64;
        let newest_hash = hex::encode(rpc.block_hash(newest as usize).await.map_err(node_err)?);

        let (mark, unsettled, ready_before, outputs_before, known) = {
            let mut state = self.inner.state.lock().await;
            // Before the newest block counts: coins it makes ready are news.
            let ready_before = state.spendable();
            state.tip = state.tip.max(newest + 1);
            let mark = state.pull_mark_height.zip(state.pull_mark_hash.clone());
            // On its way already counts as known: it's not news again once mined.
            let in_pool = self.inner.pool.lock().expect("pool lock").keys().cloned().collect::<Vec<_>>();
            let known: HashSet<String> = state.outputs.iter().map(|o| o.tx.clone()).chain(in_pool).collect();
            (mark, Self::unsettled_sent(&state), ready_before, state.outputs.len(), known)
        };
        let chain_changed = match mark {
            Some((height, hash)) if height <= newest => hex::encode(rpc.block_hash(height as usize).await.map_err(node_err)?) != hash,
            // The chain is now shorter than the block last seen.
            Some(_) => true,
            None => false,
        };

        let mut to_scan = vec![];
        if !unsettled.is_empty() {
            let hashes: Vec<&str> = unsettled.iter().map(|(tx, _)| tx.as_str()).collect();
            let lookup = self.look_up_sent(&rpc, &hashes).await?;
            for (tx, height) in &lookup.mined {
                let known = unsettled.iter().find(|(hash, _)| hash == tx).and_then(|(_, h)| *h);
                if known != Some(*height) {
                    to_scan.push(*height);
                }
            }
            self.note_missing(&lookup).await?;
        }
        to_scan.sort_unstable();
        to_scan.dedup();
        for height in to_scan {
            self.scan_blocks(height, height).await?;
        }
        if mode == PullMode::Auto {
            let blocks = u64::from(self.inner.state.lock().await.incoming_scan_blocks);
            self.scan_blocks((newest + 1).saturating_sub(blocks), newest).await?;
            // A nicety, as when syncing: a node that won't share it stops nothing.
            let _ = self.scan_pool(&rpc).await;
        }

        let mut state = self.inner.state.lock().await;
        state.pull_mark_height = Some(newest);
        state.pull_mark_hash = Some(newest_hash);
        let on_the_way: Vec<String> = self.inner.pool.lock().expect("pool lock").keys().cloned().collect();
        let new = state.outputs[outputs_before.min(state.outputs.len())..]
            .iter()
            .filter(|o| !known.contains(&o.tx) && !state.sent.iter().any(|s| s.tx == o.tx) && !state.spends.iter().any(|s| s.tx == o.tx))
            .map(|o| o.tx.as_str())
            .chain(on_the_way.iter().filter(|tx| !known.contains(*tx)).map(String::as_str))
            .collect::<HashSet<_>>()
            .len();
        let ready = state.spendable().checked_sub(ready_before).filter(|more| *more > 0).map(amount::format_short);
        let missing = state.sent.iter().filter(|s| s.not_confirmed).count();
        let words = manual::pull_words(mode, unsettled.len(), new, ready, chain_changed, missing);
        self.save_state(&state)?;
        drop(state);
        self.refresh_status().await;
        self.tell(self.status());
        Ok(words)
    }

    /// What the node knows of these sent payments.
    pub(super) async fn look_up_sent(&self, rpc: &Daemon, hashes: &[&str]) -> Result<SentLookup> {
        let body = serde_json::json!({ "txs_hashes": hashes, "decode_as_json": false }).to_string();
        let response = rpc.rpc_call("get_transactions", Some(body), 16 * 1024 * 1024).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        Ok(sent_lookup(&response))
    }

    /// Marks what the node has never heard of, a day after it was sent, as not
    /// confirmed, and puts back what it has heard of again. Only says so: the
    /// coins stay spent until someone releases them.
    pub(super) async fn note_missing(&self, lookup: &SentLookup) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if mark_missing(&mut state.sent, lookup, now()) {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// With syncing running, looks now and then for sent payments gone quiet.
    /// Only those over a day old are asked about.
    pub(super) async fn check_quiet_payments(&self, rpc: &Daemon) {
        {
            let mut checked = self.inner.quiet_checked.lock().expect("quiet lock");
            if checked.is_some_and(|at| at.elapsed() < QUIET_CHECK_EVERY) {
                return;
            }
            *checked = Some(Instant::now());
        }
        let old = now().saturating_sub(NOT_CONFIRMED_AFTER);
        let quiet: Vec<String> = self.inner.state.lock().await.sent.iter().filter(|s| s.height.is_none() && !s.released && s.time <= old).map(|s| s.tx.clone()).collect();
        if quiet.is_empty() {
            return;
        }
        let hashes: Vec<&str> = quiet.iter().map(String::as_str).collect();
        if let Ok(lookup) = self.look_up_sent(rpc, &hashes).await {
            let _ = self.note_missing(&lookup).await;
        }
    }

    /// Frees the coins of a payment marked not confirmed, to spend again. Only
    /// the node's refusal can stop the same coin going twice, so if the payment
    /// does get mined after all, the next scan shows it and any later spend fails.
    pub async fn release_payment(&self, tx: &str) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let Some(i) = state.sent.iter().position(|s| s.tx == tx && s.not_confirmed && s.height.is_none()) else {
            return Err(FobError::Screen(ScreenError::NoSuchPayment));
        };
        state.sent[i].not_confirmed = false;
        state.sent[i].released = true;
        for output in &mut state.outputs {
            if output.spent_in.as_deref() == Some(tx) {
                output.spent_in = None;
            }
        }
        self.save_state(&state)
    }

    /// Scans `from` to `to` without moving Last scan, adding what it finds to
    /// what's known. Unlike syncing, the blocks needn't follow on from anything.
    pub(super) async fn scan_blocks(&self, from: u64, to: u64) -> Result<()> {
        let _running = self.inner.scan_running.lock().await;
        let (transport, _, _, adaptive) = self.connection().await?;
        // get_blocks.bin can't start at the genesis block, which pays no one here.
        let mut at = from.max(1);
        while at <= to {
            if self.inner.scan_stop.swap(false, Ordering::Relaxed) {
                break;
            }
            let mut pipeline = Pipeline::start(transport.clone(), adaptive.clone(), at, to);
            let blocks = pipeline.next_batch(BATCH as usize).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
            if blocks.is_empty() {
                break;
            }
            let watching = Self::watched(self.inner.state.lock().await.receive_index);
            let (scanner, prefilter) = self.scanner(watching);
            let gpu_allowed = self.gpu_allowed().await;
            let (results, last) = tokio::task::spawn_blocking(move || scan::scan(&scanner, &prefilter, blocks, gpu_allowed))
                .await
                .map_err(|e| FobError::Node(format!("scanning: {e}")))?;
            let mut state = self.inner.state.lock().await;
            let mut images = Self::key_images(&state);
            let sent: HashMap<String, usize> = state.sent.iter().enumerate().map(|(i, s)| (s.tx.clone(), i)).collect();
            let mut changed = false;
            for result in results {
                // An address further on came into use: this block again, watching for it.
                if Self::watched(state.receive_index) > watching {
                    break;
                }
                let scanned = result.map_err(FobError::Node)?;
                let height = scanned.height;
                changed |= self.take_block(&mut state, &mut images, &sent, scanned);
                at = height + 1;
                self.scan_reached(at);
            }
            changed |= self.record_prices(&mut state);
            if changed {
                self.save_state(&state)?;
            }
            drop(state);
            if last.is_some() {
                *self.inner.last_block.lock().expect("block lock") = last;
            }
            self.refresh_status().await;
            self.tell(self.status());
        }
        Ok(())
    }
}

/// What a node said of some sent payments.
#[derive(Debug, Default, PartialEq)]
pub(super) struct SentLookup {
    /// Each with the height it was mined at.
    pub(super) mined: Vec<(String, u64)>,
    pub(super) in_pool: Vec<String>,
    /// Those it said outright that it doesn't have. An answer it couldn't read says nothing.
    pub(super) missing: Vec<String>,
}

/// Reads a `get_transactions` answer.
pub(super) fn sent_lookup(response: &str) -> SentLookup {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response) else { return SentLookup::default() };
    let mut lookup = SentLookup::default();
    for tx in value.get("txs").and_then(|t| t.as_array()).into_iter().flatten() {
        let Some(hash) = tx.get("tx_hash").and_then(|h| h.as_str()) else { continue };
        if tx.get("in_pool").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            lookup.in_pool.push(hash.to_owned());
        } else if let Some(height) = tx.get("block_height").and_then(serde_json::Value::as_u64) {
            lookup.mined.push((hash.to_owned(), height));
        }
    }
    lookup.missing = value.get("missed_tx").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    lookup
}

/// Applies `lookup` to the payments sent. Whether anything changed.
pub(super) fn mark_missing(sent: &mut [Sent], lookup: &SentLookup, now: u64) -> bool {
    let mut changed = false;
    for s in sent.iter_mut().filter(|s| s.height.is_none() && !s.released) {
        let quiet = lookup.missing.contains(&s.tx) && now.saturating_sub(s.time) >= NOT_CONFIRMED_AFTER;
        let heard = lookup.in_pool.contains(&s.tx) || lookup.mined.iter().any(|(tx, _)| *tx == s.tx);
        if quiet && !s.not_confirmed {
            s.not_confirmed = true;
            changed = true;
        } else if heard && s.not_confirmed {
            s.not_confirmed = false;
            changed = true;
        }
    }
    changed
}
