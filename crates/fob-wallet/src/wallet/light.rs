//! Syncing through a light wallet server (phase 2 of the Light Wallet
//! proposal, `docs/light-wallet-proposal.md`): read-only. The server holds
//! the view key and scans; each poll Fob asks it for every transaction that
//! touched the wallet and [`reconcile`]s its own state with the answer. The
//! spend key stays here: key images are worked out on this device, and an
//! output the server names is checked against the view key before it counts.

use super::*;
use crate::light::{self, Backend, LightClient, Problem, lws};
use monero_wallet::{ed25519::Scalar, extra::PaymentId};
use state::{SyncMode, TIME_LOCK_FROM};

/// How far before the server's scanned height node scanning starts again
/// on switching back: decision 9's reorganisation margin.
const BACK_MARGIN: u64 = 30;

/// The first block with compact amounts (hard fork 10, Bulletproofs 2) on
/// each network. From it on, an output's mask comes from the view key, so
/// the server's can be checked; older RingCT outputs carried theirs
/// encrypted, and it's taken on the server's word (a wrong one only makes
/// the coin unspendable, never more money).
fn compact_from(network: Network) -> u64 {
    match network {
        Network::Mainnet => 1_978_433,
        Network::Stagenet => 269_000,
    }
}

impl Wallet {
    /// Where this wallet's syncing comes from.
    pub async fn backend(&self) -> Backend {
        let state = self.inner.state.lock().await;
        match (state.sync_mode, &state.light_url) {
            (SyncMode::Light, Some(url)) => Backend::Light { url: url.clone(), self_signed: state.light_self_signed },
            _ => Backend::Node,
        }
    }

    /// Syncing through a light wallet server now.
    pub async fn light_mode(&self) -> bool {
        self.inner.state.lock().await.sync_mode == SyncMode::Light
    }

    /// The light wallet server chosen, in use or not.
    pub async fn light_server(&self) -> Option<light::Setting> {
        let state = self.inner.state.lock().await;
        state.light_url.clone().map(|url| light::Setting { url, self_signed: state.light_self_signed })
    }

    /// Settings' row: "lws.example:8443 · synced" while syncing through it,
    /// just the address while it's only chosen, nothing before one is.
    pub async fn light_line(&self) -> Option<String> {
        let server = self.light_server().await?;
        let host = node::address(&server.url);
        let state = self.light_mode().await.then(|| self.status().chip().state);
        Some(fob_words::light::line(&host, state))
    }

    /// The Light server screen's Test: `/get_version`, which sends nothing
    /// of the wallet's. A server already chosen is held to its pinned key.
    pub async fn check_light_server(&self, url: &str, self_signed: bool) -> light::Report {
        let pin = {
            let state = self.inner.state.lock().await;
            let same = state.light_url.as_deref() == Some(node::normalize(url).as_str()) && state.light_self_signed == self_signed;
            state.light_pin.as_deref().filter(|_| same).and_then(|p| hex::decode(p).ok()?.try_into().ok())
        };
        light::check(self.network(), url, self_signed, pin).await
    }

    /// Chooses the light wallet server, refusing one that doesn't answer, is
    /// on another network or has subaddresses off (`set_node`'s rules, plus
    /// that one). It isn't synced through until [`use_light_wallet`]; while
    /// it is, the new one takes over at once, and is told of the wallet afresh.
    ///
    /// [`use_light_wallet`]: Wallet::use_light_wallet
    pub async fn set_light_server(&self, url: &str, self_signed: bool) -> Result<()> {
        let url = node::normalize(url);
        node::reachable(&url).map_err(|e| {
            FobError::Light(match e {
                crate::error::EndpointProblem::Onion => Problem::Onion,
                _ => Problem::BadAddress,
            })
        })?;
        let report = light::check(self.network(), &url, self_signed, None).await;
        if let Some(refused) = report.refusal() {
            return Err(FobError::Light(refused));
        }
        let restart = {
            let mut state = self.inner.state.lock().await;
            if state.light_url.as_deref() == Some(url.as_str()) && state.light_self_signed == self_signed {
                return Ok(());
            }
            state.light_url = Some(url);
            state.light_self_signed = self_signed;
            state.light_pin = None;
            state.light_registered = false;
            state.light_subaddrs_sent = 0;
            self.save_state(&state)?;
            state.sync_mode == SyncMode::Light
        };
        *self.inner.light.lock().expect("light lock") = None;
        if restart {
            self.restart_syncing().await;
        }
        Ok(())
    }

    /// Syncs through the chosen light wallet server, or through the node
    /// again. Turning it on keeps what was found (outputs, spends, payments
    /// sent), forgets the block hashes kept for reorganisations (the server
    /// follows those now) and has the server told of the wallet afresh.
    /// Turning it off has node scanning start again from the server's last
    /// scanned block less [`BACK_MARGIN`], never before the wallet's first.
    /// What the server found stays meanwhile: the node finds it again and
    /// takes it as known (`take_block`'s rule), so the balance doesn't drop
    /// while it catches up those blocks.
    pub async fn use_light_wallet(&self, on: bool) -> Result<()> {
        {
            let mut state = self.inner.state.lock().await;
            match (on, state.sync_mode) {
                (true, SyncMode::Light) | (false, SyncMode::Node) => return Ok(()),
                (true, SyncMode::Node) => {
                    if state.light_url.is_none() {
                        return Err(FobError::Light(Problem::NoServerSet));
                    }
                    state.sync_mode = SyncMode::Light;
                    state.recent.clear();
                    state.light_registered = false;
                    state.light_subaddrs_sent = 0;
                    self.save_state(&state)?;
                }
                (false, SyncMode::Light) => {
                    state.sync_mode = SyncMode::Node;
                    // Never answered: node scanning carries on where it stopped.
                    if state.light_scanned_height > 0 {
                        state.next_block = state.light_scanned_height.saturating_sub(BACK_MARGIN).max(state.restore_height.unwrap_or(0));
                        // No block before it is known to follow: the first batch isn't checked.
                        state.recent.clear();
                    }
                    self.save_state(&state)?;
                }
            }
        }
        *self.inner.pipeline.lock().await = None;
        self.inner.pool.lock().expect("pool lock").clear();
        self.inner.pool_seen.lock().expect("pool lock").clear();
        *self.inner.light.lock().expect("light lock") = None;
        self.set_status(|s| s.light = on.then(light_view_default));
        self.restart_syncing().await;
        self.restart_zmq().await;
        Ok(())
    }

    /// Starts the sync loop again on what's chosen now (a node, a server,
    /// light mode on or off), dropping what was connected; just tells the
    /// app if it wasn't running. What `set_node` does after switching.
    pub(super) async fn restart_syncing(&self) {
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        let looping = self.inner.syncing.lock().expect("sync lock").is_some();
        self.stop_sync();
        *self.inner.rpc.lock().await = None;
        *self.inner.link.lock().expect("link lock") = None;
        *self.inner.pipeline.lock().await = None;
        *self.inner.light.lock().expect("light lock") = None;
        *self.inner.node_verdict.lock().expect("node lock") = None;
        let status = self.set_status(|s| {
            s.error = None;
            s.error_local = false;
            s.connected = false;
            s.waiting_seconds = 0;
        });
        self.refresh_status().await;
        if let Some(listener) = listener {
            if looping {
                self.start_sync(listener);
            } else {
                listener.on_status(status);
            }
        }
    }

    /// The server's client, made once per server.
    async fn light_client(&self) -> Result<LightClient> {
        let (setting, pin) = {
            let state = self.inner.state.lock().await;
            let Some(url) = state.light_url.clone() else { return Err(FobError::Light(Problem::NoServerSet)) };
            let pin: Option<[u8; 32]> = state.light_pin.as_deref().and_then(|p| hex::decode(p).ok()?.try_into().ok());
            (light::Setting { url, self_signed: state.light_self_signed }, pin)
        };
        if let Some((kept, client)) = self.inner.light.lock().expect("light lock").as_ref()
            && *kept == setting
        {
            return Ok(client.clone());
        }
        let client = LightClient::pinned(&setting.url, setting.self_signed, pin).map_err(FobError::Light)?;
        *self.inner.light.lock().expect("light lock") = Some((setting, client.clone()));
        Ok(client)
    }

    /// Keeps the key a self-signed server answered with first.
    async fn keep_light_pin(&self, client: &LightClient) -> Result<()> {
        let Some(pin) = client.pin() else { return Ok(()) };
        let mut state = self.inner.state.lock().await;
        if state.light_self_signed && state.light_pin.is_none() {
            state.light_pin = Some(hex::encode(pin));
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// The protocol's only credential: the address and its private view key.
    fn light_login(&self) -> lws::Login {
        lws::Login { address: self.inner.keys.primary_address(), view_key: lws::Hex(*self.inner.keys.view_bytes()) }
    }

    /// The server hasn't approved this wallet (yet): said, and asked again at
    /// the next poll, `/login` first.
    async fn waiting_for_approval(&self) -> Result<bool> {
        {
            let mut state = self.inner.state.lock().await;
            if state.light_registered {
                state.light_registered = false;
                self.save_state(&state)?;
            }
        }
        self.set_status(|s| {
            s.connected = true;
            if !s.error_local {
                s.error = None;
            }
            s.light.get_or_insert_with(light_view_default).waiting_approval = true;
        });
        self.refresh_status().await;
        Ok(false)
    }

    /// One poll of the server: registered, told of the addresses watched,
    /// asked for the wallet's transactions, and what it said reconciled.
    /// Always answers that nothing more is waiting: the loop polls again in 20 s.
    pub(super) async fn light_step(&self) -> Result<bool> {
        let client = self.light_client().await?;
        let login = self.light_login();
        let refused = |e: Problem| FobError::Light(e);

        // 1. Registered.
        let (registered, restored) = {
            let state = self.inner.state.lock().await;
            (state.light_registered, state.restored)
        };
        if !registered {
            match client.login(&login, true, !restored).await {
                Err(Problem::Forbidden) => return self.waiting_for_approval().await,
                Err(e) => return Err(refused(e)),
                Ok(answer) => {
                    self.keep_light_pin(&client).await?;
                    let import = {
                        let mut state = self.inner.state.lock().await;
                        state.light_registered = true;
                        let start = answer.start_height;
                        let import = match (state.restore_height, start) {
                            // A new wallet that never synced starts where the server does.
                            (None, Some(start)) => {
                                state.restore_height = Some(start);
                                state.next_block = start;
                                false
                            }
                            // Blocks between what Fob scanned itself and where the
                            // server starts would go unscanned: ask for them.
                            (Some(_), Some(start)) => start > state.next_block,
                            _ => false,
                        };
                        self.save_state(&state)?;
                        import
                    };
                    self.set_status(|s| s.light.get_or_insert_with(light_view_default).waiting_approval = false);
                    if import {
                        let said = match client.import_wallet_request(&login).await {
                            Ok(answer) => answer.status,
                            Err(Problem::NotTaken) => fob_words::light::IMPORT_NOT_OFFERED.to_owned(),
                            Err(Problem::Forbidden) => return self.waiting_for_approval().await,
                            Err(e) => return Err(refused(e)),
                        };
                        self.set_status(|s| s.light.get_or_insert_with(light_view_default).import = Some(said));
                    }
                }
            }
        }

        // 2. The addresses watched: account 0, minors 1 to watched().
        let (watched, told) = {
            let state = self.inner.state.lock().await;
            (Self::watched(state.receive_index), state.light_subaddrs_sent)
        };
        if watched > told {
            match client.upsert_subaddrs(&login, vec![lws::SubaddrRanges { key: 0, value: vec![[1, watched]] }]).await {
                Ok(_) => {
                    let mut state = self.inner.state.lock().await;
                    state.light_subaddrs_sent = state.light_subaddrs_sent.max(watched);
                    self.save_state(&state)?;
                }
                Err(Problem::Forbidden) => return self.waiting_for_approval().await,
                Err(e) => return Err(refused(e)),
            }
        }

        // 3. Its transactions, reconciled.
        let answer = match client.get_address_txs(&login).await {
            Ok(answer) => answer,
            Err(Problem::Forbidden) => return self.waiting_for_approval().await,
            Err(e) => return Err(refused(e)),
        };
        let pool_before = self.inner.pool.lock().expect("pool lock").clone();
        let rejected = {
            let mut state = self.inner.state.lock().await;
            if state.sync_mode != SyncMode::Light {
                // Switched back while asking.
                return Ok(false);
            }
            let done = reconcile(&self.inner.keys, &mut state, &answer, &pool_before, now());
            *self.inner.pool.lock().expect("pool lock") = done.pool;
            // 5. Prices for payments newly seen, as the node's step records them.
            let priced = self.record_prices(&mut state);
            if done.changed || priced {
                self.save_state(&state)?;
            }
            done.refused
        };

        // 4. Status.
        self.set_status(|s| {
            s.connected = true;
            if !s.error_local {
                s.error = None;
            }
            s.light.get_or_insert_with(light_view_default).waiting_approval = false;
        });
        self.refresh_status().await;
        if rejected > 0 {
            return Err(FobError::Light(Problem::BadOutputs(rejected)));
        }
        Ok(false)
    }
}

/// What the chip knows of a server before it has answered.
fn light_view_default() -> catch_up::Light {
    catch_up::Light::default()
}

/// What a poll's reconciliation came to.
#[derive(Debug)]
pub(super) struct Reconciled {
    /// Anything kept in the file changed.
    pub(super) changed: bool,
    /// Payments to this wallet in the pool, as the node's pool scan keeps them.
    pub(super) pool: HashMap<String, Pooled>,
    /// Outputs the server named that failed the checks: not counted.
    pub(super) refused: usize,
}

/// Brings `state` in line with what the server says of the wallet. The
/// server's list is the truth for the blocks it vouches for (from its start
/// height to its scanned height); each rule is idempotent, so the same answer
/// twice changes nothing the second time.
///
/// - **Gone**: a transaction Fob recorded at a height the server vouches
///   for and no longer lists was reorganised out: its outputs and spends go,
///   a payment sent loses its height, and coins it spent are freed unless
///   a spend still known or a payment of this wallet's holds them (`rewind`'s rule).
/// - **Received**: each output listed whose one-time key isn't known yet
///   becomes owned (the first with a key wins: the burning bug), once its key
///   checks against the view key and, for an output with compact amounts,
///   its mask too. Locked to a time: dropped, as decision 48 had it (there's
///   no chain clock here to judge it by).
/// - **Spent**: a candidate spend whose key image is one of the wallet's
///   (worked out with the spend key, as `take_block` does) spends that
///   output. The API gives no fee, so the spend's is 0.
/// - **Sent**: a payment of this wallet's that's listed gets its height.
/// - **Pool**: transactions listed with `mempool: true` are the pool, kept
///   in memory; their outputs aren't owned until mined.
pub(super) fn reconcile(keys: &Keys, state: &mut State, answer: &lws::AddressTxs, pool_before: &HashMap<String, Pooled>, now: u64) -> Reconciled {
    let mut changed = false;
    let mut refused = 0;
    let mut mined: Vec<&lws::Transaction> = answer.transactions.iter().filter(|t| !t.mempool && t.height.is_some()).collect();
    mined.sort_by_key(|t| t.height);
    let listed: HashSet<String> = mined.iter().map(|t| hex::encode(t.hash.0)).collect();
    // Strictly below its scanned height: monokulo-lws gives the next block it will
    // scan there, monero-lws the last one it did, and either way this is scanned.
    let vouched = |height: u64| height >= answer.start_height && height < answer.scanned_height;

    // Gone.
    let before = (state.outputs.len(), state.spends.len());
    state.outputs.retain(|o| !vouched(o.height) || listed.contains(&o.tx));
    state.spends.retain(|s| !vouched(s.height) || listed.contains(&s.tx));
    changed |= before != (state.outputs.len(), state.spends.len());
    for sent in &mut state.sent {
        if sent.height.is_some_and(|h| vouched(h) && !listed.contains(&sent.tx)) {
            sent.height = None;
            changed = true;
        }
    }
    let holding: HashSet<String> = state.spends.iter().map(|s| s.tx.clone()).chain(state.sent.iter().filter(|s| !s.released).map(|s| s.tx.clone())).collect();
    for output in &mut state.outputs {
        if output.spent_in.as_ref().is_some_and(|t| !hardware::is_sync_mark(t) && !holding.contains(t)) {
            output.spent_in = None;
            changed = true;
        }
    }

    // Received.
    for tx in &mined {
        let height = tx.height.unwrap_or_default();
        let hash = hex::encode(tx.hash.0);
        let time = tx.timestamp.as_ref().and_then(|t| unix_time(&t.0)).unwrap_or(0);
        let unlock_block = match tx.unlock_time {
            0 => 0,
            block if block < TIME_LOCK_FROM => block,
            // Locked to a time.
            _ => continue,
        };
        for received in &tx.receives {
            let key = hex::encode(received.public_key.0);
            if state.outputs.iter().any(|o| o.key == key) {
                continue;
            }
            let Some(output) = checked_output(keys, tx, received, height, unlock_block) else {
                refused += 1;
                continue;
            };
            let subaddress = received.recipient.min_i;
            // Money to the split address doesn't move Receive on to it.
            if Some(subaddress) != state.split_index {
                state.receive_index = state.receive_index.max(subaddress);
            }
            state.outputs.push(Owned {
                output: hex::encode(output.serialize()),
                tx: hash.clone(),
                height,
                time,
                amount: received.amount.0,
                subaddress,
                coinbase: tx.coinbase,
                unlock_block,
                key,
                key_image: keys.key_image(output.key(), output.key_offset()).map(hex::encode),
                spent_in: None,
                // The key it was derived from: the transaction's own, or the
                // extra one at its index; a device takes either.
                tx_key: Some(hex::encode(received.tx_pub_key.0)),
                extra_key: None,
            });
            changed = true;
        }
    }

    // Spent, and sent.
    let images = Wallet::key_images(state);
    for tx in &mined {
        let height = tx.height.unwrap_or_default();
        let hash = hex::encode(tx.hash.0);
        let time = tx.timestamp.as_ref().and_then(|t| unix_time(&t.0)).unwrap_or(0);
        for candidate in &tx.spent_outputs {
            let Some(&i) = images.get(&candidate.key_image.0) else { continue };
            if !state.spends.iter().any(|s| s.tx == hash) {
                state.spends.push(ChainSpend { tx: hash.clone(), height, time, fee: 0 });
                changed = true;
            }
            if state.outputs[i].spent_in.as_deref() != Some(hash.as_str()) {
                state.outputs[i].spent_in = Some(hash.clone());
                changed = true;
            }
        }
        if let Some(sent) = state.sent.iter_mut().find(|s| s.tx == hash)
            && (sent.height != Some(height) || sent.not_confirmed || sent.released || sent.signed.is_some())
        {
            sent.height = Some(height);
            // It went through after all.
            sent.not_confirmed = false;
            sent.released = false;
            sent.signed = None;
            changed = true;
        }
    }

    // The pool.
    let mut pool: HashMap<String, Pooled> = HashMap::new();
    for tx in answer.transactions.iter().filter(|t| t.mempool) {
        let hash = hex::encode(tx.hash.0);
        // A payment of this wallet's: its change isn't money coming in.
        if state.sent.iter().any(|s| s.tx == hash) || state.outputs.iter().any(|o| o.tx == hash) {
            continue;
        }
        for received in &tx.receives {
            if received.recipient.maj_i != 0 || keys.light_output(&received.tx_pub_key.0, received.index.into(), received.recipient.min_i, &received.public_key.0).is_none() {
                refused += 1;
                continue;
            }
            let seen = pool_before.get(&hash).map_or(now, |p| p.seen);
            let pooled = pool.entry(hash.clone()).or_insert(Pooled { seen, parts: vec![] });
            add_part(&mut pooled.parts, received.recipient.min_i, received.amount.0);
        }
    }

    if state.light_scanned_height != answer.scanned_height || state.tip != answer.blockchain_height {
        state.light_scanned_height = answer.scanned_height;
        state.tip = answer.blockchain_height;
        changed = true;
    }
    Reconciled { changed, pool, refused }
}

/// The `WalletOutput` for an output the server says this wallet received,
/// once it checks: its one-time key is the one the view key makes from the
/// transaction key and index for the subaddress named, and its mask is the
/// one the view key makes (from hard fork 10 on; a mined reward's is 1).
/// The amount can't be checked: the server gives no commitment from the
/// chain (the 96-byte `rct` blob of `/get_unspent_outs` carries one it
/// works out itself from the same mask and amount), so a server could
/// misstate an amount. A wrong amount can't be spent: the node refuses the
/// transaction.
pub(super) fn checked_output(keys: &Keys, tx: &lws::Transaction, received: &lws::ReceivedOutput, height: u64, unlock_block: u64) -> Option<WalletOutput> {
    if received.recipient.maj_i != 0 {
        return None;
    }
    let index = u64::from(received.index);
    let (key_offset, derived_mask) = keys.light_output(&received.tx_pub_key.0, index, received.recipient.min_i, &received.public_key.0)?;
    let said = Scalar::read(&mut received.rct?.0.as_slice()).ok()?;
    let mask = match tx.coinbase {
        true if said == monero_wallet::ed25519::Commitment::zero().mask => said,
        true => return None,
        false if said == derived_mask || height < compact_from(keys.network) => said,
        false => return None,
    };
    let payment_id = match tx.payment_id.as_ref().map(|p| p.0.as_slice()) {
        Some(id) if id.len() == 8 => Some(PaymentId::Encrypted(id.try_into().ok()?)),
        _ => None,
    };
    let timelock = if unlock_block == 0 { Timelock::None } else { Timelock::Block(usize::try_from(unlock_block).ok()?) };
    let bytes = wallet_output_bytes(&WalletOutputParts {
        tx: tx.hash.0,
        index,
        global_index: received.global_index,
        key: received.public_key.0,
        key_offset,
        mask,
        amount: received.amount.0,
        timelock,
        subaddress: SubaddressIndex::new(0, received.recipient.min_i),
        payment_id,
    });
    WalletOutput::read(&mut bytes.as_slice()).ok()
}

/// What a `WalletOutput` is made of.
pub(super) struct WalletOutputParts {
    pub(super) tx: [u8; 32],
    pub(super) index: u64,
    pub(super) global_index: u64,
    pub(super) key: [u8; 32],
    pub(super) key_offset: Scalar,
    pub(super) mask: Scalar,
    pub(super) amount: u64,
    pub(super) timelock: Timelock,
    pub(super) subaddress: Option<SubaddressIndex>,
    pub(super) payment_id: Option<PaymentId>,
}

/// A `WalletOutput` as monero-wallet 0.2 writes one (`output.rs`), which has
/// no public constructor: transaction hash ‖ index in it (u64 LE) ‖ global
/// index (u64 LE) ‖ one-time key ‖ key offset ‖ commitment (mask ‖ amount
/// u64 LE) ‖ timelock ‖ subaddress flag (+ account u32 LE, address u32 LE)
/// ‖ payment id flag (+ the id) ‖ how many pieces of arbitrary data (0).
/// "Not protocol defined", in monero-oxide's words: a test pins it to what
/// its scanner writes.
pub(super) fn wallet_output_bytes(parts: &WalletOutputParts) -> Vec<u8> {
    let mut out = Vec::with_capacity(200);
    out.extend_from_slice(&parts.tx);
    out.extend_from_slice(&parts.index.to_le_bytes());
    out.extend_from_slice(&parts.global_index.to_le_bytes());
    out.extend_from_slice(&parts.key);
    parts.key_offset.write(&mut out).expect("writing to a Vec");
    monero_wallet::ed25519::Commitment::new(parts.mask, parts.amount).write(&mut out).expect("writing to a Vec");
    parts.timelock.write(&mut out).expect("writing to a Vec");
    match parts.subaddress {
        Some(sub) => {
            out.push(1);
            out.extend_from_slice(&sub.account().to_le_bytes());
            out.extend_from_slice(&sub.address().to_le_bytes());
        }
        None => out.push(0),
    }
    match parts.payment_id {
        Some(id) => {
            out.push(1);
            id.write(&mut out).expect("writing to a Vec");
        }
        None => out.push(0),
    }
    // No arbitrary data: a varint 0.
    out.push(0);
    out
}

/// Seconds since the epoch from `YYYY-MM-DDTHH:MM:SSZ`, as the API writes times.
pub(super) fn unix_time(text: &str) -> Option<u64> {
    let (date, time) = text.trim().trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|p| p.split('.').next().unwrap_or("").parse::<i64>());
    let (h, min, s) = (t.next()?.ok()?, t.next()?.ok()?, t.next().and_then(|s| s.ok()).unwrap_or(0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + min * 60 + s).ok()
}
