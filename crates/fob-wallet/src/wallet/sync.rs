//! Following the chain: the sync loop, scanning, the pool and the GPU check.

use super::*;
use coin::{Coin, Event, Hold, KeyImage, TxHash};

/// How long leaving waits for the loop to let go of the wallet. An aborted
/// task drops what it holds on the runtime's thread, and when that is the
/// last clone of a wallet, its database closes there. A process exiting
/// meanwhile (a test binary, say) runs SQLCipher's own shutdown on the main
/// thread at the same time, and that crashes. So the caller waits, a few
/// milliseconds as a rule, and a bounded while at worst.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// The pool's transactions fetched at once.
const POOL_CHUNK: usize = 100;

/// Waits, up to `timeout`, for tasks that hold a clone of a wallet to end,
/// aborted or finished. Safe from inside a runtime: the waiting is done on
/// Fob's own runtime, and a worker of a multi-thread runtime steps aside
/// while it blocks.
fn settle(tasks: Vec<tokio::task::JoinHandle<()>>, timeout: Duration) {
    if tasks.is_empty() {
        return;
    }
    let (done, finished) = std::sync::mpsc::channel();
    crate::runtime().spawn(async move {
        for task in tasks {
            let _ = task.await;
        }
        let _ = done.send(());
    });
    let wait = || {
        let _ = finished.recv_timeout(timeout);
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => tokio::task::block_in_place(wait),
        _ => wait(),
    }
}

impl Wallet {
    pub fn status(&self) -> SyncStatus {
        self.inner.status.lock().expect("status lock").clone()
    }

    pub(super) fn set_status(&self, f: impl FnOnce(&mut SyncStatus)) -> SyncStatus {
        let mut status = self.inner.status.lock().expect("status lock");
        f(&mut status);
        status.clone()
    }

    /// Hears the status now, if anyone's listening.
    pub(super) fn tell(&self, status: SyncStatus) {
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        if let Some(listener) = listener {
            listener.on_status(status);
        }
    }

    /// A scanner watching up to `watching`, from the last one.
    pub(super) fn scanner(&self, watching: u32) -> (Scanner, Arc<fob_scanner::Prefilter>) {
        let pair = self.inner.keys.view_pair();
        let mut kept = self.inner.scanner.lock().expect("scanner lock");
        let (upto, scanner, prefilter) = kept.get_or_insert_with(|| {
            (0, Scanner::new(pair.clone()), Arc::new(fob_scanner::Prefilter::new(&pair, &self.inner.keys.view_bytes())))
        });
        if *upto < watching {
            Self::watch(scanner, *upto + 1, watching);
            // Scans under way hold the last one; this is only ever a little behind them.
            let mut grown = (**prefilter).clone();
            for i in *upto + 1..=watching {
                if let Some(sub) = SubaddressIndex::new(0, i) {
                    grown.watch(&pair, sub);
                }
            }
            *prefilter = Arc::new(grown);
            *upto = watching;
        }
        (scanner.clone(), prefilter.clone())
    }

    /// Whether this wallet may scan on the GPU now, running its check first if
    /// that's due. False until the GPU has passed this cold start's check.
    pub(super) async fn gpu_allowed(&self) -> bool {
        let Some(device) = fob_scanner::gpu_identity() else { return false };
        {
            let state = self.inner.state.lock().await;
            let checked = self.inner.gpu_checked.load(Ordering::Relaxed);
            match gpu_check_due(state.gpu_checked_on.as_deref(), state.gpu_check_passed, &device, checked) {
                GpuCheck::Passed => return true,
                GpuCheck::Failed => return false,
                GpuCheck::Due => {}
            }
        }
        let view = self.inner.keys.view_bytes();
        let Ok(Some(passed)) = tokio::task::spawn_blocking(move || fob_scanner::check_wallet(&view)).await else { return false };
        self.inner.gpu_checked.store(true, Ordering::Relaxed);
        let mut state = self.inner.state.lock().await;
        state.gpu_checked_on = Some(device);
        state.gpu_check_passed = passed;
        // Unsaved, the check runs again next time: nothing is lost.
        let _ = self.save_state(&state);
        passed
    }

    /// Whether this wallet's GPU check last failed, so the CPU scans it.
    pub async fn gpu_check_failed(&self) -> bool {
        let state = self.inner.state.lock().await;
        state.gpu_checked_on.is_some() && !state.gpu_check_passed
    }

    /// Scans the next batch of blocks. Returns whether more are waiting.
    ///
    /// The scan runs on the phone's cores with the state left free, so the
    /// screens never wait on it; what it found is applied under the lock, in
    /// block order.
    pub async fn sync_step(&self) -> Result<bool> {
        match self.backend().await {
            crate::light::Backend::Node => self.node_step().await,
            crate::light::Backend::Light { .. } => self.light_step().await,
        }
    }

    /// [`sync_step`](Wallet::sync_step) from the node: fetch blocks, scan them.
    async fn node_step(&self) -> Result<bool> {
        let (transport, rpc, decoys, adaptive) = self.connection().await?;
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));

        let (mut start, mut tip, recent) = {
            let state = self.inner.state.lock().await;
            (state.next_block, state.tip, state.recent.last().cloned())
        };
        let before = self.status();
        // Ask for the tip only near the end of what's known, or first thing
        // in a run, when what's known may be old: while catching up, every
        // request goes to fetching blocks.
        if tip == 0 || start + BATCH > tip || !before.connected {
            tip = rpc.latest_block_number().await.map_err(node_err)? as u64 + 1;
            if let Some(job) = self.inner.scan_job.lock().expect("scan lock").as_mut() {
                job.to = job.to.max(tip);
            }
            let mut state = self.inner.state.lock().await;
            state.tip = tip;
            if state.restore_height.is_none() {
                // A new wallet can't have money in blocks before it existed.
                let first = tip.saturating_sub(UNLOCK);
                state.restore_height = Some(first);
                state.next_block = first;
                self.save_state(&state)?;
            } else {
                self.inner.unsaved.store(true, Ordering::Relaxed);
            }
            start = state.next_block;
        }
        self.refresh_status().await;
        let status = self.set_status(|s| {
            s.connected = true;
            // A chain that changed too far back stays said until a round gets past it.
            if !s.error_local && !s.chain_changed {
                s.error = None;
            }
        });
        // The node has answered: Synced from last time is put right, or
        // Connecting ends, now, not once the first blocks are in and scanned.
        if !before.connected || before.synced != status.synced {
            self.tell(status);
        }
        if start >= tip {
            // Caught up: show payments on their way before they're mined. The
            // pool is a nicety: a node that won't share it doesn't stop syncing.
            let _ = self.scan_pool(&rpc).await;
            self.publish_unanswered(&rpc).await;
            self.check_quiet_payments(&rpc).await;
            // And keep the decoy data current, so a payment is quick to prepare.
            if self.can_spend() {
                let _ = decoys.update(tip as usize - 1).await;
            }
            self.flush().await;
            return Ok(false);
        }

        let Batch { blocks, hashes } = if start == 0 {
            // get_blocks.bin can't start at the genesis block; fetch it alone.
            Batch::of(rpc.contiguous_scannable_blocks(0..=0).await.map_err(node_err)?)
        } else {
            // Keep fetching ahead while this step scans: reuse the pipeline
            // if it's where the wallet is, else start one to the tip.
            let mut pipeline = self.inner.pipeline.lock().await;
            if pipeline.as_ref().is_none_or(|p| p.next != start || p.next > p.end) {
                *pipeline = Some(Pipeline::start(transport.clone(), adaptive.clone(), start, tip - 1));
            }
            let running = pipeline.as_mut().expect("just started");
            // A slow node can keep this waiting for minutes, each request
            // retried several times: the app hears how long.
            let asked = Instant::now();
            let batch = loop {
                match tokio::time::timeout(WAIT_TICK, running.next_batch(BATCH as usize)).await {
                    Ok(batch) => break batch,
                    Err(_) => {
                        let status = self.set_status(|s| s.waiting_seconds = asked.elapsed().as_secs());
                        self.tell(status);
                    }
                }
            };
            self.set_status(|s| s.waiting_seconds = 0);
            match batch {
                Ok(batch) if !batch.blocks.is_empty() => batch,
                Ok(_) => {
                    // Ran out before the tip moved on: start again next step.
                    *pipeline = None;
                    return Ok(true);
                }
                Err(e) => {
                    *pipeline = None;
                    return Err(node_err(e));
                }
            }
        };
        // A reorganisation shows as a first block that doesn't follow the last
        // one scanned: scan the recent blocks again, if that's far enough back.
        if let (Some((_, hash)), Some(first)) = (&recent, blocks.first())
            && hex::encode(first.block.header.previous) != *hash
        {
            *self.inner.pipeline.lock().await = None;
            self.on_reorg(&rpc).await?;
            return Ok(true);
        }

        let watching = Self::watched(self.inner.state.lock().await.receive_index);
        let (scanner, prefilter) = self.scanner(watching);
        let gpu_allowed = self.gpu_allowed().await;
        let (results, last) = tokio::task::spawn_blocking(move || scan::scan(&scanner, &prefilter, blocks, &hashes, gpu_allowed))
            .await
            .map_err(|e| FobError::Node(format!("scanning: {e}")))?;

        let mut state = self.inner.state.lock().await;
        if state.next_block != start {
            // Rewound or reset while fetching or scanning: this batch is stale.
            return Ok(true);
        }
        let mut images = Self::key_images(&state);
        let sent = Self::sent_txs(&state);
        let mut changed = false;
        let mut watch_further = false;
        for result in results {
            // A payment near the edge, or an address just handed out, moves it
            // on from this block. What's left of the batch was scanned watching
            // too few: it's fetched and scanned again next step.
            if Self::watched(state.receive_index) > watching {
                watch_further = true;
                break;
            }
            let scanned = result.map_err(FobError::Node)?;
            let (height, time) = (scanned.height, scanned.time);
            let hash = scanned.hash.clone();
            changed |= self.take_block(&mut state, &mut images, &sent, scanned);
            self.scan_reached(height + 1);

            state.recent.push((height, hash));
            let keep = manual::keep_hashes(state.settled_confirmations);
            while state.recent.len() > keep {
                state.recent.remove(0);
            }
            state.next_block = height + 1;
            state.last_block_time = time;
        }
        if last.is_some() {
            *self.inner.last_block.lock().expect("block lock") = last;
        }
        changed |= self.record_prices(&mut state);
        let more = state.next_block < tip;
        // Money found goes straight to the file. Otherwise only so often: the
        // whole file is written each time, and the chain has it if it's lost.
        let due = self.inner.saved_at.lock().expect("save lock").elapsed() >= SAVE_EVERY;
        if changed || !more || due {
            self.save_state(&state)?;
        } else {
            self.inner.unsaved.store(true, Ordering::Relaxed);
        }
        drop(state);
        if watch_further {
            *self.inner.pipeline.lock().await = None;
        }
        self.refresh_status().await;
        Ok(more)
    }

    /// Looks for payments to this wallet in the node's pool, so they show
    /// as on their way about two minutes before they're mined.
    ///
    /// monero-oxide scans blocks, so the pool's transactions are scanned in a
    /// stand-in: the last block scanned, with the pool's transactions as its
    /// own. Only their amounts are kept, in memory; the real block scans them
    /// again once mined, with their true output indexes.
    pub(super) async fn scan_pool(&self, rpc: &Daemon) -> Result<()> {
        let node_err = |e: InterfaceError| FobError::Node(format!("{e:?}"));
        let response = rpc.rpc_call("get_transaction_pool_hashes", None, 4 * 1024 * 1024).await.map_err(node_err)?;
        let hashes: Vec<String> = serde_json::from_str::<serde_json::Value>(&response)
            .ok()
            .and_then(|v| v.get("tx_hashes").and_then(|h| serde_json::from_value(h.clone()).ok()))
            .unwrap_or_default();
        let in_pool: HashSet<&str> = hashes.iter().map(String::as_str).collect();
        self.inner.pool.lock().expect("pool lock").retain(|tx, _| in_pool.contains(tx.as_str()));
        self.inner.pool_seen.lock().expect("pool lock").retain(|tx| in_pool.contains(tx.as_str()));
        {
            // A coin a pool transaction held, and the pool no longer has the
            // transaction: it left unmined (one mined in a block not yet
            // scanned is spent again when the scan gets there). What a
            // device's sync found names no transaction, and `step` leaves
            // it be (decisions 364 and 399).
            let mut state = self.inner.state.lock().await;
            let gone: Vec<(usize, TxHash)> = state
                .outputs
                .iter()
                .enumerate()
                .filter_map(|(i, o)| match o.coin {
                    Coin::Held { hold: Hold::InPool { tx }, .. } if !in_pool.contains(hex::encode(tx).as_str()) => Some((i, tx)),
                    _ => None,
                })
                .collect();
            let mut changed = false;
            for (i, tx) in gone {
                changed |= state.apply_at(i, Event::PoolLeft { tx });
            }
            if changed {
                self.save_state(&state)?;
            }
        }

        let new: Vec<[u8; 32]> = {
            let state = self.inner.state.lock().await;
            let ours: HashSet<&str> = state.sent.iter().map(|s| s.tx.as_str()).collect();
            let seen = self.inner.pool_seen.lock().expect("pool lock");
            hashes
                .iter()
                .filter(|h| !seen.contains(*h) && !ours.contains(h.as_str()))
                .filter_map(|h| hex::decode(h).ok()?.try_into().ok())
                .collect()
        };
        if new.is_empty() {
            return Ok(());
        }
        let last = self.inner.last_block.lock().expect("block lock").clone();
        let block = match last {
            Some(block) => block,
            // Opened already synced: nothing scanned yet this run.
            None => {
                let latest = rpc.latest_block_number().await.map_err(node_err)?;
                let block = rpc.block_by_number(latest).await.map_err(node_err)?;
                *self.inner.last_block.lock().expect("block lock") = Some(block.clone());
                block
            }
        };
        let miner = block.miner_transaction().hash();
        let receive_index = self.inner.state.lock().await.receive_index;
        // The kept scanner: working out every watched address again is slow.
        let (scanner, _) = self.scanner(Self::watched(receive_index));
        // A busy pool is thousands of transactions: a chunk at a time, each
        // seen once it's scanned, so one that fails leaves only the rest to
        // fetch again.
        for chunk in new.chunks(POOL_CHUNK) {
            let (hashes, txs) = pool_transactions(rpc, chunk).await?;
            // Spending this wallet's coins, however it got there: a payment the
            // node gave Fob no answer for, or one from another wallet with these
            // keys. As wallet2 does, a payment in the pool holds its coins until
            // it's mined or leaves.
            {
                let mut state = self.inner.state.lock().await;
                let images = Self::key_images(&state);
                let mut changed = false;
                for (hash, tx) in hashes.iter().zip(&txs) {
                    for image in spent_images(tx) {
                        if let Some(&i) = images.get(&image) {
                            changed |= state.apply_at(i, Event::PoolSpend { tx: *hash });
                        }
                    }
                }
                if changed {
                    self.save_state(&state)?;
                }
            }
            let mut block = block.clone();
            block.transactions = hashes;
            let stand_in = monero_wallet::interface::ScannableBlock {
                block,
                transactions: txs.into_iter().map(Into::into).collect(),
                output_index_for_first_ringct_output: Some(0),
            };
            let mut scanner = scanner.clone();
            let found = tokio::task::spawn_blocking(move || scanner.scan(stand_in))
                .await
                .map_err(|e| FobError::Node(format!("scanning the pool: {e}")))?
                .map_err(|e| FobError::Node(format!("scanning the pool: {e:?}")))?;
            let outputs = found.ignore_additional_timelock().into_iter().filter(|o| o.transaction() != miner);
            self.pool_found(outputs.map(|o| (hex::encode(o.transaction()), o.subaddress().map(|s| s.address()), o.commitment().amount)), now());
            // Those that left the pool too: they're not looked for again.
            self.inner.pool_seen.lock().expect("pool lock").extend(chunk.iter().map(hex::encode));
        }
        // A payment first seen in the pool is priced as it's seen.
        let mut state = self.inner.state.lock().await;
        if self.record_prices(&mut state) {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// Keeps what a pool scan found, `seen` now: each output's transaction,
    /// the subaddress it paid (`None` for the main address, index 0) and
    /// its amount, each address's own part of a transaction kept apart.
    pub(super) fn pool_found(&self, outputs: impl IntoIterator<Item = (String, Option<u32>, u64)>, seen: u64) {
        let mut pool = self.inner.pool.lock().expect("pool lock");
        for (tx, subaddress, amount) in outputs {
            let pooled = pool.entry(tx).or_insert(Pooled { seen, parts: vec![] });
            add_part(&mut pooled.parts, subaddress.unwrap_or(0), amount);
        }
    }

    /// Brings [`status`](Wallet::status) up to date with the wallet's state.
    pub(super) async fn refresh_status(&self) {
        let state = self.inner.state.lock().await;
        if state.sync_mode == state::SyncMode::Light {
            // The server's scanning is what's said: its scanned height and the chain's.
            let host = node::address(state.light_url.as_deref().unwrap_or(""));
            let tip = state.tip;
            // Its next block, never past the chain's (monokulo-lws already gives the next).
            let scanned = (state.light_scanned_height + 1).min(tip.max(1));
            let start = state.restore_height.unwrap_or(0);
            let network = self.network();
            drop(state);
            self.set_status(|s| {
                s.scanned = scanned;
                s.tip = tip;
                s.start = start;
                s.manual_sync = false;
                s.scan = None;
                s.synced = tip > 0 && scanned >= tip;
                s.progress = if tip <= start { 1.0 } else { (scanned.saturating_sub(start)) as f32 / (tip - start) as f32 };
                let light = s.light.get_or_insert_with(catch_up::Light::default);
                light.host = host;
                light.scanning_from = (tip.saturating_sub(scanned) >= catch_up::SERVER_FAR_BEHIND).then(|| network.timestamp_for(scanned));
            });
            return;
        }
        let manual_sync = state.manual_sync;
        let (next, tip, start, up_to) = (state.next_block, state.tip, state.restore_height.unwrap_or(state.next_block), state.last_block_time);
        drop(state);
        let scan = self.inner.scan_job.lock().expect("scan lock").clone();
        // Manual Sync takes the wallet as up to date, but while it's asked to scan.
        let scanning = self.inner.scan_asked.load(Ordering::Relaxed);
        self.set_status(|s| {
            s.scanned = next;
            s.tip = tip;
            s.start = start;
            s.manual_sync = manual_sync;
            s.scan = scan;
            s.synced = (manual_sync && !scanning) || (tip > 0 && next >= tip);
            s.progress = if tip <= start { 1.0 } else { (next.saturating_sub(start)) as f32 / (tip - start) as f32 };
            s.up_to = up_to;
            s.light = None;
        });
    }

    /// Forgets the blocks from `from` on, to scan them again. `anchor`, the
    /// block before them, is what the next one must follow.
    pub(super) async fn rewind(&self, from: u64, anchor: Option<(u64, String)>) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        state.spends.retain(|s| s.height < from);
        state.rewind_coins(from);
        for sent in &mut state.sent {
            if sent.height.is_some_and(|h| h >= from) {
                sent.height = None;
            }
        }
        state.recent = anchor.into_iter().collect();
        state.next_block = from;
        // A pool payment's coins just freed above are marked again by the
        // next look at the pool, which looks at every transaction afresh.
        self.inner.pool_seen.lock().expect("pool lock").clear();
        *self.inner.pipeline.lock().await = None;
        self.save_state(&state)
    }

    /// Tells `listener` where syncing is, measuring this catch-up's `pace`
    /// as it goes (a failure starts it again, measuring nothing).
    pub(super) async fn report(&self, listener: &dyn SyncListener, pace: &mut catch_up::Pace, failed: Option<&FobError>) {
        self.refresh_status().await;
        let (next, tip) = {
            let status = self.status();
            (status.scanned, status.tip)
        };
        let link = self.inner.link.lock().expect("link lock").clone();
        let received = link.as_ref().map_or(0, |(_, transport)| transport.received());
        let (seconds_left, speeds) = pace.report(Instant::now(), received, next, tip.saturating_sub(next), failed.is_some());
        // As the Node checks would say it, the certificate included.
        let error = failed.map(|e| match &link {
            Some((setting, transport)) => sync_failure(e, transport.certificate(), setting.self_signed),
            None => sync_failure(e, None, false),
        });
        if error.as_ref().is_some_and(|(_, local)| !local) {
            // A failing sync is newer than the last check of the node.
            *self.inner.node_verdict.lock().expect("node lock") = None;
        }
        let status = self.set_status(|s| {
            s.seconds_left = seconds_left;
            s.blocks_per_second = speeds.map(|s| s.blocks_per_second);
            s.link_mb_per_second = speeds.map(|s| s.bytes_per_second / 1e6);
            s.error_local = error.as_ref().is_some_and(|(_, local)| *local);
            s.chain_changed = matches!(failed, Some(FobError::ReorgTooDeep));
            if failed.is_some() {
                s.connected = false;
            }
            s.error = error.map(|(words, _)| words);
        });
        listener.on_status(status);
    }

    /// The card Home's sync chip opens, for the status now.
    pub fn sync_details(&self, clock: Clock) -> catch_up::Details {
        catch_up::details(&self.status(), clock)
    }

    /// The latest changes to the wallet's coins, oldest first, one line
    /// each (`coin 1a2b3c4d… Held(DeviceChain asked at 100) --PoolLeft
    /// 5e6f…--> Free`): why the balance moved, for the sync card's detail.
    /// The last thousand of this run; not saved.
    pub async fn coin_transitions(&self) -> Vec<String> {
        self.inner.state.lock().await.transitions.iter().map(ToString::to_string).collect()
    }

    /// Keeps the wallet up to date in the background until [`stop_sync`]:
    /// catches up as fast as the node allows, then looks for new blocks
    /// every 20 s, or every 2 minutes while the node's ZMQ publisher is
    /// heard (a new block then wakes it at once). With instant updates on,
    /// the subscriber runs beside it. `listener` hears every step.
    ///
    /// [`stop_sync`]: Wallet::stop_sync
    pub fn start_sync(&self, listener: Arc<dyn SyncListener>) {
        self.stop_sync();
        *self.inner.listener.lock().expect("listener lock") = Some(listener.clone());
        // Whatever it said last, from minutes or days ago, it doesn't know now.
        let status = self.set_status(|s| {
            s.connected = false;
            s.waiting_seconds = 0;
            if !s.error_local {
                s.error = None;
            }
        });
        listener.on_status(status);
        let wallet = self.clone();
        let handle = crate::runtime().spawn(async move {
            let mut pace = catch_up::Pace::default();
            let mut backoff = Duration::from_secs(2);
            let mut reported: Option<Instant> = None;
            loop {
                // Manual Sync: nothing looks for blocks until asked.
                if wallet.waiting_to_be_asked().await {
                    wallet.report(listener.as_ref(), &mut pace, None).await;
                    wallet.inner.rounds.send_modify(|n| *n += 1);
                    wallet.wait(Duration::from_secs(3600), listener.as_ref()).await;
                    continue;
                }
                match wallet.sync_step().await {
                    Ok(more) => {
                        backoff = Duration::from_secs(2);
                        // Often enough to look alive: every report has the app
                        // read its screens again, on the time scanning could use.
                        if !more || reported.is_none_or(|at| at.elapsed() >= REPORT_EVERY) {
                            wallet.report(listener.as_ref(), &mut pace, None).await;
                            reported = Some(Instant::now());
                        }
                        if !more {
                            pace.reset();
                            fob_scanner::rest();
                            wallet.scan_finished().await;
                            wallet.inner.rounds.send_modify(|n| *n += 1);
                            if wallet.inner.state.lock().await.manual_scanning() {
                                continue;
                            }
                            let poll = if wallet.status().zmq == ZmqStatus::Listening { POLL_LISTENING } else { POLL };
                            wallet.wait(poll, listener.as_ref()).await;
                        }
                    }
                    Err(e) => {
                        wallet.report(listener.as_ref(), &mut pace, Some(&e)).await;
                        wallet.inner.rounds.send_modify(|n| *n += 1);
                        wallet.wait(backoff, listener.as_ref()).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                    }
                }
            }
        });
        *self.inner.syncing.lock().expect("sync lock") = Some(handle);
        // Beside the loop, while instant updates are on.
        let wallet = self.clone();
        crate::runtime().spawn(async move { wallet.restart_zmq().await });
    }

    pub fn stop_sync(&self) {
        fob_scanner::rest();
        let mut ending = vec![];
        if let Some(handle) = self.inner.syncing.lock().expect("sync lock").take() {
            handle.abort();
            ending.push(handle);
        }
        if let Some((endpoint, task)) = self.inner.zmq_task.lock().expect("zmq lock").take() {
            task.abort();
            ending.push(task);
            // Still on: it connects again with the loop.
            self.set_zmq_status(ZmqStatus::Connecting, Some(&endpoint));
        }
        *self.inner.listener.lock().expect("listener lock") = None;
        // Leaving Fob, or this wallet: the scanning since the last save stays.
        if self.inner.unsaved.load(Ordering::Relaxed) {
            let wallet = self.clone();
            ending.push(crate::runtime().spawn(async move { wallet.flush().await }));
        }
        settle(ending, STOP_WAIT);
    }

    /// The sync loop's wait between rounds, cut short by [`sync_now`](Wallet::sync_now)
    /// or a new block announced. A transaction announced in the pool has
    /// the pool looked at (once caught up) and the wait go on.
    pub(super) async fn wait(&self, time: Duration, listener: &dyn SyncListener) {
        let deadline = tokio::time::Instant::now() + time;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return,
                _ = self.inner.wake.notified() => return,
                _ = self.inner.pool_wake.notified() => {
                    tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + POOL_GAP)).await;
                    if self.status().synced && self.status().error.is_none() && let Ok(rpc) = self.rpc().await {
                        let _ = self.scan_pool(&rpc).await;
                        self.report(listener, &mut catch_up::Pace::default(), None).await;
                    }
                }
            }
        }
    }

    /// Asks the node for new blocks and its pool now, rather than at the next
    /// poll (pull to refresh). Returns once that round is over: caught up,
    /// failed (the error is in [`status`](Wallet::status)), or 20 s passed.
    /// Without the sync loop running it does one step itself.
    pub async fn sync_now(&self) {
        let running = self.inner.syncing.lock().expect("sync lock").as_ref().is_some_and(|h| !h.is_finished());
        if !running {
            let _ = tokio::time::timeout(SYNC_NOW_WAIT, self.sync_step()).await;
            self.refresh_status().await;
            return;
        }
        let mut rounds = self.inner.rounds.subscribe();
        rounds.mark_unchanged();
        // A round already under way ends this wait; the stored permit then
        // starts the next one at once, which is what pulling asked for.
        self.inner.wake.notify_one();
        let _ = tokio::time::timeout(SYNC_NOW_WAIT, rounds.changed()).await;
    }

    /// Key images of the coins known, each to its place: looked up for every
    /// input of every transaction in a batch.
    pub(super) fn key_images(state: &State) -> HashMap<[u8; 32], usize> {
        let mut images = HashMap::with_capacity(state.outputs.len());
        for (i, output) in state.outputs.iter().enumerate() {
            if let Some(image) = output.coin.key_image() {
                images.entry(image).or_insert(i);
            }
        }
        images
    }

    /// Payments sent, each to its place: looked up for every transaction in a batch.
    pub(super) fn sent_txs(state: &State) -> HashMap<[u8; 32], usize> {
        state.sent.iter().enumerate().filter_map(|(i, s)| Some((hash_bytes(&s.tx)?, i))).collect()
    }

    /// Takes in what a scanned block holds for this wallet. Whether it changed
    /// anything. The same block twice changes nothing the second time.
    pub(super) fn take_block(&self, state: &mut State, images: &mut HashMap<[u8; 32], usize>, sent: &HashMap<[u8; 32], usize>, scanned: scan::Scanned) -> bool {
        let keys = &self.inner.keys;
        let (height, time) = (scanned.height, scanned.time);
        let mut changed = false;
        let mut outgoing: HashSet<String> = HashSet::new();
        let mut incoming = false;
        for seen in &scanned.txs {
            let mut ours = false;
            if let Some(&i) = sent.get(&seen.hash) {
                state.sent[i].height = Some(height);
                // It went through after all.
                state.sent[i].not_confirmed = false;
                state.sent[i].released = false;
                state.sent[i].signed = None;
                changed = true;
                ours = true;
            }
            for image in &seen.key_images {
                if let Some(&i) = images.get(image) {
                    let tx = hex::encode(seen.hash);
                    if !state.spends.iter().any(|s| s.tx == tx) {
                        state.spends.push(ChainSpend { tx, height, time, fee: seen.fee });
                        changed = true;
                    }
                    changed |= state.apply_at(i, Event::MinedSpend { tx: seen.hash, height });
                    ours = true;
                }
            }
            if ours {
                outgoing.insert(hex::encode(seen.hash));
            }
        }

        for output in scanned.found {
            // Locked to a block (a mined reward, rarely anything else) or to
            // a time: kept, and not spent before then. A time is kept as
            // Monero writes it, past the highest block number; Fob judges it
            // by the chain's own clock, the blocks' times, not the phone's,
            // which is never earlier than monerod's clock-plus-two-hours rule.
            let unlock_block = match output.additional_timelock() {
                Timelock::None => 0,
                Timelock::Block(block) => block as u64,
                Timelock::Time(time) => time.max(state::TIME_LOCK_FROM),
            };
            let tx = hex::encode(output.transaction());
            let key = hex::encode(output.key().compress().to_bytes());
            // Seen before: the same output again, or the burning bug (a
            // second output with a key already received, which can't
            // both be spent). Only the first counts; one kept before Fob
            // recorded transaction keys takes them now, for a device.
            if let Some(known) = state.outputs.iter_mut().find(|o| o.key == key || (o.tx == tx && o.output == hex::encode(output.serialize()))) {
                if known.tx_key.is_none()
                    && let Some((tx_key, extra)) = scanned.tx_keys.get(&tx).and_then(|(tx_keys, additional)| keys.tx_key_for(&output, tx_keys, additional.as_deref()))
                {
                    known.tx_key = Some(hex::encode(tx_key));
                    known.extra_key = extra.map(hex::encode);
                    changed = true;
                }
                continue;
            }
            self.inner.pool.lock().expect("pool lock").remove(&tx);
            // Change comes in a transaction this wallet sent: it isn't money in.
            incoming |= !outgoing.contains(&tx) && !state.sent.iter().any(|s| s.tx == tx);
            let subaddress = output.subaddress().map(|s| s.address()).unwrap_or(0);
            // Money to the split address doesn't move Receive on to it.
            if Some(subaddress) != state.split_index {
                state.receive_index = state.receive_index.max(subaddress);
            }
            // A view-only wallet can't work the key image out, nor can a
            // device's until the device is asked; a rewind may have kept
            // what the device said of this coin before.
            let ki = match keys.key_image(output.key(), output.key_offset()) {
                Some(image) => KeyImage::Known(image),
                None => KeyImage::Unknown,
            };
            let serialized = hex::encode(output.serialize());
            let coin = state.found_coin(&(tx.clone(), serialized.clone()), ki);
            if let Some(image) = coin.key_image() {
                images.entry(image).or_insert(state.outputs.len());
            }
            let (tx_key, extra_key) = scanned
                .tx_keys
                .get(&tx)
                .and_then(|(tx_keys, additional)| keys.tx_key_for(&output, tx_keys, additional.as_deref()))
                .map_or((None, None), |(tx_key, extra)| (Some(hex::encode(tx_key)), extra.map(hex::encode)));
            // An output arriving in a transaction already seen spending (our
            // own change) is still ours to spend later.
            state.outputs.push(Owned {
                output: serialized,
                coinbase: tx == scanned.miner_tx,
                unlock_block,
                key,
                tx,
                height,
                time,
                amount: output.commitment().amount,
                subaddress,
                coin,
                tx_key,
                extra_key,
            });
            changed = true;
        }
        if incoming || !outgoing.is_empty() {
            self.scan_mark(height, incoming);
        }
        changed
    }

    /// Decides how to answer a reorganisation, by asking the node whether the
    /// oldest hash kept is still on its chain, and answers it. Refuses one that
    /// reaches past that hash: scanning only the recent blocks would leave
    /// what came from the old chain before them in the wallet, unnoticed.
    pub(super) async fn on_reorg(&self, rpc: &Daemon) -> Result<()> {
        let (oldest, start) = {
            let state = self.inner.state.lock().await;
            let Some(oldest) = state.recent.first().cloned() else { return Ok(()) };
            (oldest, state.restore_height.unwrap_or(0))
        };
        let on_chain = rpc.block_hash(oldest.0 as usize).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        match reorg_reach(oldest.0, hex::encode(on_chain) == oldest.1, start) {
            ReorgReach::Kept => self.rewind(oldest.0 + 1, Some(oldest)).await?,
            ReorgReach::ToStart => self.rewind(start, None).await?,
            ReorgReach::TooDeep => return Err(FobError::ReorgTooDeep),
        }
        self.set_status(|s| s.notice = Some(catch_up::Notice::ChainFollowed));
        Ok(())
    }

    /// Scans the whole wallet again, from its first block: the way past a
    /// reorganisation too deep to check. Keeps what only this wallet knows
    /// (contacts, notes, what was sent), and comes back with what's on the chain.
    pub async fn scan_again(&self) -> Result<()> {
        let start = self.inner.state.lock().await.restore_height.unwrap_or(0);
        *self.inner.pipeline.lock().await = None;
        self.rewind(start, None).await?;
        self.ask_to_scan(ScanTitle::WholeWallet, start).await;
        self.sync_now().await;
        Ok(())
    }
}

/// The pool's transactions `hashes`, with their hashes, less any that left
/// the pool since it named them (mined, or dropped): the node turns down
/// the whole call for one of those, so then they're asked for one by one.
async fn pool_transactions(rpc: &Daemon, hashes: &[[u8; 32]]) -> Result<(Vec<[u8; 32]>, Vec<monero_wallet::transaction::Transaction>)> {
    let node_err = |e: TransactionsError| FobError::Node(format!("{e:?}"));
    match rpc.transactions(hashes).await {
        Ok(txs) => Ok((hashes.to_vec(), txs)),
        Err(TransactionsError::TransactionNotFound) => {
            let (mut found, mut txs) = (vec![], vec![]);
            for hash in hashes {
                match rpc.transactions(std::slice::from_ref(hash)).await {
                    Ok(tx) => {
                        found.push(*hash);
                        txs.extend(tx);
                    }
                    Err(TransactionsError::TransactionNotFound) => {}
                    Err(e) => return Err(node_err(e)),
                }
            }
            Ok((found, txs))
        }
        Err(e) => Err(node_err(e)),
    }
}

/// The key images of the coins `tx` spends.
fn spent_images(tx: &monero_wallet::transaction::Transaction) -> Vec<[u8; 32]> {
    use monero_wallet::transaction::Input;
    tx.prefix()
        .inputs
        .iter()
        .filter_map(|input| match input {
            Input::ToKey { key_image, .. } => Some(key_image.to_bytes()),
            Input::Gen(_) => None,
        })
        .collect()
}

/// How far back a reorganisation reaches, against the hashes kept.
#[derive(Debug, PartialEq)]
pub(super) enum ReorgReach {
    /// The oldest hash kept is still on the chain: only what's after it changed.
    Kept,
    /// It's gone, but it's the wallet's first block: nothing older is known.
    ToStart,
    /// It's gone, and older blocks were scanned that may have changed too.
    TooDeep,
}

pub(super) fn reorg_reach(oldest: u64, still_on_chain: bool, wallet_start: u64) -> ReorgReach {
    match still_on_chain {
        true => ReorgReach::Kept,
        false if oldest <= wallet_start => ReorgReach::ToStart,
        false => ReorgReach::TooDeep,
    }
}

/// Why a payment wasn't published.
#[derive(Debug)]
pub(super) enum Unpublished {
    /// The node turned it down, or was never reached: nothing left the wallet.
    Refused(FobError),
    /// The node gave no answer to go on (it timed out, the line dropped
    /// mid-answer): it may have taken the payment and passed it on.
    Unsure,
}

/// Why publishing failed. With Manual Sync on, a refusal may mean a coin was
/// spent since the last scan, whatever words the node used, so it's told apart.
/// Only the node's own refusal, or a request that never reached it, says the
/// payment didn't go.
pub(super) fn publish_failure(e: monero_wallet::interface::PublishTransactionError, manual_sync: bool) -> Unpublished {
    use monero_wallet::interface::PublishTransactionError as E;
    match e {
        // Many nodes give no reason.
        E::TransactionRejected(reason) if manual_sync => Unpublished::Refused(FobError::SpendRejected(if reason.trim().is_empty() { String::new() } else { format!(": {}", reason.trim()) })),
        e @ E::TransactionRejected(_) => Unpublished::Refused(FobError::Node(format!("{e:?}"))),
        e => {
            let text = format!("{e:?}");
            if transport::never_reached(&text) { Unpublished::Refused(FobError::Node(text)) } else { Unpublished::Unsure }
        }
    }
}
