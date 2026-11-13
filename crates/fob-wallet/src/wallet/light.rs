//! Syncing through a light wallet server (phase 2 of the Light Wallet
//! proposal, `docs/light-wallet-proposal.md`): read-only. The server holds
//! the view key and scans; each poll Fob asks it for every transaction that
//! touched the wallet and [`reconcile`]s its own state with the answer. The
//! spend key stays here: key images are worked out on this device, and an
//! output the server names is checked against the view key before it counts,
//! and, when the server proves its transaction is in a block, against the
//! block a node has there.

use super::*;
use coin::{Coin, Event, Hold, KeyImage, TxHash};
use crate::light::{
    self, Backend, LightClient, Problem,
    hashes::{Known, NodeHashes},
    lws, proof,
};
use catch_up::Amounts;
use std::collections::BTreeMap;
use monero_wallet::{ed25519::Scalar, extra::PaymentId};
use state::{SyncMode, TIME_LOCK_FROM};

/// How far before the server's scanned height node scanning starts again
/// on switching back: decision 9's reorganisation margin.
const BACK_MARGIN: u64 = 30;

/// The first block with compact amounts (hard fork 10, Bulletproofs 2) on
/// each network. From it on, an output's mask comes from the view key, so
/// the server's can be checked; older RingCT outputs carried theirs
/// encrypted, and it's the server's, held to the output's commitment when
/// the server gives one (a wrong one only makes the coin unspendable,
/// never more money).
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

    /// What the Light server screen shows of the server in use before any
    /// Test: the key a server whose own certificate is accepted is held to,
    /// what the amounts it reports are worth, and why its last answer
    /// failed, if it did.
    pub async fn light_standing(&self) -> fob_words::light::Standing {
        let pin: Option<[u8; 32]> = {
            let state = self.inner.state.lock().await;
            state.light_pin.as_deref().filter(|_| state.light_self_signed).and_then(|p| hex::decode(p).ok()?.try_into().ok())
        };
        let status = self.status();
        let (light, failed) = match &status.light {
            Some(light) => (light.clone(), status.error.clone()),
            None => (catch_up::Light::default(), None),
        };
        fob_words::light::standing(pin.as_ref(), &light, failed.as_deref())
    }

    /// The Light server screen's Test: `/get_version`, which sends nothing
    /// of the wallet's. A server already chosen is held to its pinned key.
    pub async fn check_light_server(&self, url: &str, self_signed: bool) -> light::Report {
        let pin = {
            let state = self.inner.state.lock().await;
            let same = state.light_url.as_deref() == Some(light::normalize(url).as_str()) && state.light_self_signed == self_signed;
            state.light_pin.as_deref().filter(|_| same).and_then(|p| hex::decode(p).ok()?.try_into().ok())
        };
        light::check(self.network(), url, self_signed, pin).await
    }

    /// Chooses the light wallet server, refusing one that doesn't answer, is
    /// on another network or has subaddresses off (`set_node`'s rules, plus
    /// that one), or is plain `http://` to another machine. It isn't synced
    /// through until [`use_light_wallet`]; while it is, the new one takes
    /// over at once, and is told of the wallet afresh. A server whose own
    /// certificate is accepted is held from here on to the key this check
    /// saw, so the view key's first journey is to that key too.
    ///
    /// [`use_light_wallet`]: Wallet::use_light_wallet
    pub async fn set_light_server(&self, url: &str, self_signed: bool) -> Result<()> {
        let url = light::normalize(url);
        light::allowed(&url).map_err(FobError::Light)?;
        let report = light::check(self.network(), &url, self_signed, None).await;
        if let Some(refused) = report.refusal() {
            return Err(FobError::Light(refused));
        }
        let restart = {
            let mut state = self.inner.state.lock().await;
            if state.light_url.as_deref() == Some(url.as_str()) && state.light_self_signed == self_signed {
                // Chosen again with another key: the new one is accepted, as
                // choosing a node again accepts its new certificate.
                let pin = report.pin.map(hex::encode);
                if !self_signed || state.light_pin == pin {
                    return Ok(());
                }
                state.light_pin = pin;
                self.save_state(&state)?;
                drop(state);
                *self.inner.light.lock().expect("light lock") = None;
                return Ok(());
            }
            state.light_url = Some(url);
            state.light_self_signed = self_signed;
            state.light_pin = report.pin.map(hex::encode);
            state.light_registered = false;
            state.light_subaddrs_sent = 0;
            state.light_accounts_sent.clear();
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
                    state.light_accounts_sent.clear();
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
        self.reset_pipeline().await;
        self.inner.pool.lock().expect("pool lock").clear();
        self.inner.pool_seen.lock().expect("pool lock").clear();
        *self.inner.light.lock().expect("light lock") = None;
        self.set_status(|s| s.light = on.then(light_view_default));
        self.restart_syncing().await;
        self.restart_zmq().await;
        Ok(())
    }

    /// Whether a light wallet server is asked to scan the blocks between
    /// where this wallet starts and where the server starts it (an import,
    /// which monokulo-lws and monero-lws do from block 0, and may hold for
    /// the operator to approve). On by default. An app whose wallet is new,
    /// with nothing to find before the server's start, turns it off: Till,
    /// whose wallet starts at the tip when it's set up (decision 425).
    /// Not kept in the file: said again each time the wallet is opened.
    pub fn set_light_import(&self, on: bool) {
        self.inner.light_no_import.store(!on, Ordering::Relaxed);
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
        self.reset_pipeline().await;
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
    pub(super) async fn light_client(&self) -> Result<LightClient> {
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
    pub(super) fn light_login(&self) -> lws::Login {
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
                            // server starts would go unscanned: ask for them, unless
                            // the wallet wants nothing before the server's start.
                            (Some(_), Some(start)) => !self.inner.light_no_import.load(Ordering::Relaxed) && start > state.next_block,
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

        // 2. The addresses watched: account 0's minors 1 to watched(), and
        // each other account's that scanning watches (decision 418).
        let (plan, told) = {
            let state = self.inner.state.lock().await;
            let mut told: HashMap<u32, u32> = state.light_accounts_sent.iter().copied().collect();
            told.insert(0, state.light_subaddrs_sent);
            (state.watch_plan(), told)
        };
        let ranges: Vec<lws::SubaddrRanges> = plan
            .0
            .iter()
            .filter(|(account, upto)| told.get(account).is_none_or(|t| t < upto))
            .map(|(&account, &upto)| lws::SubaddrRanges { key: account, value: vec![[if account == 0 { 1 } else { 0 }, upto]] })
            .collect();
        if !ranges.is_empty() {
            match client.upsert_subaddrs(&login, ranges).await {
                Ok(_) => {
                    let mut state = self.inner.state.lock().await;
                    for (&account, &upto) in &plan.0 {
                        if account == 0 {
                            state.light_subaddrs_sent = state.light_subaddrs_sent.max(upto);
                        } else {
                            match state.light_accounts_sent.iter_mut().find(|(a, _)| *a == account) {
                                Some((_, sent)) => *sent = (*sent).max(upto),
                                None => state.light_accounts_sent.push((account, upto)),
                            }
                        }
                    }
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
        // 3b. Its proofs checked and its outputs opened, off the runtime and
        // before the state lock: what the last poll opened isn't again.
        let last = std::mem::take(&mut *self.inner.light_opened.lock().expect("light lock"));
        let wallet = self.clone();
        let (answer, checked) = tokio::task::spawn_blocking(move || {
            let (checked, opened) = check(&wallet.inner.keys, &answer, last);
            *wallet.inner.light_opened.lock().expect("light lock") = opened;
            (answer, checked)
        })
        .await
        .map_err(|e| FobError::Node(format!("checking the server's answer: {e}")))?;
        // 3c. The blocks its proofs name, as a node has them.
        let vouched = self.vouch(&answer, &checked).await?;
        let pool_before = self.inner.pool.lock().expect("pool lock").clone();
        let done = {
            let mut state = self.inner.state.lock().await;
            if state.sync_mode != SyncMode::Light {
                // Switched back while asking.
                return Ok(false);
            }
            match reconcile_checked(&self.inner.keys, &mut state, &answer, &checked, &pool_before, now(), &vouched.known) {
                Ok(mut done) => {
                    // Only a pool that changed is a change: the activity is kept until one is.
                    let mut pool = self.inner.pool.lock().expect("pool lock");
                    if **pool != done.pool {
                        pool.set(std::mem::take(&mut done.pool));
                    }
                    drop(pool);
                    // 5. Prices for payments newly seen, as the node's step records them.
                    let priced = self.record_prices(&mut state);
                    if done.changed || priced {
                        self.save_state(&state)?;
                    }
                    done
                }
                // A lie or a contradiction: nothing of the answer was taken.
                Err(untrue) => {
                    self.set_status(|s| {
                        s.connected = true;
                        let light = s.light.get_or_insert_with(light_view_default);
                        light.refused = true;
                        vouched.say(light);
                        if matches!(untrue, Problem::AmountMismatch | Problem::NotOnChain) {
                            light.amounts = Amounts::Mismatch;
                        }
                    });
                    return Err(FobError::Light(untrue));
                }
            }
        };
        let rejected = done.refused;
        // Payments the server gave no answer for, offered again.
        self.publish_unanswered_through(&client).await;

        // 4. Status.
        self.set_status(|s| {
            s.connected = true;
            if !s.error_local {
                s.error = None;
            }
            let light = s.light.get_or_insert_with(light_view_default);
            light.waiting_approval = false;
            light.refused = false;
            vouched.say(light);
            // The weakest of this wallet's outputs says it for them all.
            light.amounts = match (done.unchecked, done.checked, done.pending, done.proven, light.amounts) {
                (1.., ..) => Amounts::ServersWord,
                (0, 1.., ..) => Amounts::Checked,
                (0, 0, 1.., ..) => Amounts::ChainPending,
                (0, 0, 0, 1.., _) => Amounts::Chain,
                (0, 0, 0, 0, Amounts::Mismatch) => Amounts::NotYet,
                (0, 0, 0, 0, before) => before,
            };
        });
        self.refresh_status().await;
        if rejected > 0 {
            return Err(FobError::Light(Problem::BadOutputs(rejected)));
        }
        Ok(false)
    }
}

impl Wallet {
    /// The node a light server's proofs are checked with: the wallet's
    /// own, or, for a wallet with none, Fob's default for its network
    /// (`true`), with the key a self-signed one is held to.
    pub(super) async fn proof_node(&self) -> (node::Setting, Option<[u8; 32]>, bool) {
        let state = self.inner.state.lock().await;
        if state.node.trim().is_empty() {
            let url = node::normalize(self.network().default_nodes()[0]);
            return (node::Setting { url, self_signed: false }, None, true);
        }
        let pin = state.node_pin.as_deref().and_then(|p| hex::decode(p).ok()?.try_into().ok());
        (node::Setting { url: state.node.clone(), self_signed: state.node_self_signed }, pin, false)
    }

    /// Asks a node for the blocks the server's proofs name (each proof that
    /// holds: one that doesn't refuses the answer in [`vet`]) for this
    /// wallet's transactions, those paying it or spending its coins, over
    /// stretches of the chain (`fob_chain::light::hashes`), from what's
    /// kept: a poll after one that checked them asks nothing. A block a
    /// server makes up for a transaction that isn't the wallet's, or past
    /// the tip, is none of the node's business. A node that can't be
    /// reached isn't the server's fault: its outputs are taken as not
    /// checked yet, and asked about again at the next poll.
    async fn vouch(&self, answer: &lws::AddressTxs, checked: &Checked) -> Result<Vouched> {
        let (mut known, first, images) = {
            let state = self.inner.state.lock().await;
            (known_of(&state), state.restore_height.unwrap_or(answer.start_height), Wallet::key_images(&state))
        };
        let mut wanted: BTreeMap<u64, [u8; 32]> = BTreeMap::new();
        for (t, tx) in answer.transactions.iter().enumerate() {
            let Some(Some(Ok(Some(proven)))) = checked.proofs.get(t) else { continue };
            let ours = checked.opened[t].iter().any(|o| matches!(o, Opened::Ours { .. })) || tx.spent_outputs.iter().any(|s| images.contains_key(&s.key_image.0) || checked.images.contains(&s.key_image.0));
            if ours && proven.height <= answer.blockchain_height.saturating_add(TIP_SLACK) {
                wanted.entry(proven.height).or_insert(proven.block);
            }
        }
        let (setting, pin, default) = self.proof_node().await;
        let before = known.clone();
        let mut reached = None;
        if !wanted.is_empty() {
            let tip = answer.blockchain_height;
            reached = Some(match Transport::pinned(&setting.url, setting.self_signed, pin) {
                Err(_) => false,
                Ok(transport) => {
                    let source = NodeHashes { transport: transport.clone(), network: self.network() };
                    let learnt = known.learn(&source, &wanted, first, tip).await.is_ok();
                    if learnt && pin.is_none() && !default {
                        self.keep_pin(&setting, &transport).await?;
                    }
                    learnt
                }
            });
            // Only what this answer's payments need is kept.
            known.blocks.retain(|h, _| wanted.contains_key(h));
        }
        if known != before {
            let mut state = self.inner.state.lock().await;
            keep_known(&mut state, &known);
            self.save_state(&state)?;
        }
        Ok(Vouched { known, host: node::address(&setting.url), default, reached })
    }
}

/// What a node said of the blocks a server's proofs name, and of itself.
pub(super) struct Vouched {
    pub(super) known: Known,
    host: String,
    default: bool,
    /// Whether the node answered, if it was asked.
    reached: Option<bool>,
}

impl Vouched {
    /// Says on the Light server screen which node checks the proofs.
    fn say(&self, light: &mut catch_up::Light) {
        light.proof_node = Some(self.host.clone());
        light.proof_node_default = self.default;
        light.proof_node_unreached = self.reached == Some(false);
    }
}

/// The block hashes kept in `state`, as `hashes` works with them.
pub(super) fn known_of(state: &State) -> Known {
    let read = |list: &[(u64, String)]| -> Vec<(u64, [u8; 32])> { list.iter().filter_map(|(h, hash)| Some((*h, hex::decode(hash).ok()?.try_into().ok()?))).collect() };
    Known { blocks: read(&state.light_blocks).into_iter().collect(), history: read(&state.light_history) }
}

/// Keeps `known` in `state`, in hex.
pub(super) fn keep_known(state: &mut State, known: &Known) {
    state.light_blocks = known.blocks.iter().map(|(h, hash)| (*h, hex::encode(hash))).collect();
    state.light_history = known.history.iter().map(|(h, hash)| (*h, hex::encode(hash))).collect();
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
    /// Outputs of this wallet's in blocks, listed with the chain's
    /// commitment, which their amount opens, but no proof; and listed
    /// without either (their amount is the server's word).
    pub(super) checked: usize,
    pub(super) unchecked: usize,
    /// Outputs of this wallet's in blocks with a proof that holds: whose
    /// block a node has (`proven`), or that no node has vouched for yet.
    pub(super) proven: usize,
    pub(super) pending: usize,
}

/// How far past the chain's tip, as the server gives it, a transaction's
/// block may be before the answer is taken to contradict itself: the tip it
/// reports can lag the block its scanner just took in.
const TIP_SLACK: u64 = 10;

/// Brings `state` in line with what the server says of the wallet. The
/// server's list is the truth for the blocks it vouches for (from its start
/// height to its scanned height); each rule is idempotent, so the same answer
/// twice changes nothing the second time.
///
/// First the whole answer is [`vet`]ted: an amount that doesn't open the
/// commitment the server gives with it, or an answer that contradicts
/// itself, refuses all of it, and nothing in `state` changes. Then:
///
/// - **Gone**: a transaction Fob recorded at a height the server vouches
///   for and no longer lists was reorganised out: its outputs and spends go,
///   a payment sent loses its height, and coins it spent are freed unless
///   a spend still known or a payment of this wallet's holds them (`rewind`'s rule).
/// - **Received**: each output listed whose one-time key isn't known yet
///   becomes owned (the first with a key wins: the burning bug), once its key
///   checks against the view key and, for an output with compact amounts,
///   its mask too. Locked to a block or to a time, it's kept with its
///   transaction's unlock time, as the node's scan keeps it.
/// - **Spent**: a candidate spend whose key image is one of the wallet's
///   (worked out with the spend key, as `take_block` does) spends that
///   output, with the fee the server gives (0, not known, from a server
///   that gives none).
/// - **Sent**: a payment of this wallet's that's listed gets its height.
/// - **Not confirmed**: a payment of this wallet's neither mined nor in the
///   pool a day after it was sent is marked so, as the node path marks one
///   its node never heard of; its coins can then be released.
/// - **Pool**: transactions listed with `mempool: true` are the pool, kept
///   in memory; their outputs aren't owned until mined. A device wallet's
///   coin whose key image a pool transaction spends is held by that
///   transaction, and let go when the pool no longer has it.
/// - **Clock**: the latest block time among the transactions it vouches
///   for moves the chain's clock on (it judges time locks), unless it's
///   past `now`, the phone's: a server can't make a time lock pass early
///   unless the phone's clock is wrong too. It never moves back here.
#[cfg(test)]
pub(super) fn reconcile(keys: &Keys, state: &mut State, answer: &lws::AddressTxs, pool_before: &HashMap<String, Pooled>, now: u64) -> std::result::Result<Reconciled, Problem> {
    reconcile_proven(keys, state, answer, pool_before, now, &Known::default())
}

/// `reconcile`, with the blocks a node vouched for: each proof the server
/// gives must name the block `chain` has at its height, or the answer is
/// refused; a proof whose block `chain` doesn't have is taken, but not
/// counted as checked.
#[cfg(test)]
pub(super) fn reconcile_proven(keys: &Keys, state: &mut State, answer: &lws::AddressTxs, pool_before: &HashMap<String, Pooled>, now: u64, chain: &Known) -> std::result::Result<Reconciled, Problem> {
    let (checked, _) = check(keys, answer, OpenCache::default());
    reconcile_checked(keys, state, answer, &checked, pool_before, now, chain)
}

/// [`reconcile_proven`], with the answer already [`check`]ed: nothing here
/// opens an output again, so it's quick under the state lock.
pub(super) fn reconcile_checked(keys: &Keys, state: &mut State, answer: &lws::AddressTxs, opened: &Checked, pool_before: &HashMap<String, Pooled>, now: u64, chain: &Known) -> std::result::Result<Reconciled, Problem> {
    let Tally { checked, unchecked, proven, pending } = vet(answer, opened, chain)?;
    let mut changed = false;
    let mut refused = 0;
    // With each one's place in the answer, for what `opened` says of it.
    let mut mined: Vec<(usize, &lws::Transaction)> = answer.transactions.iter().enumerate().filter(|(_, t)| !t.mempool && t.height.is_some()).collect();
    mined.sort_by_key(|(_, t)| t.height);
    let listed: HashSet<String> = mined.iter().map(|(_, t)| hex::encode(t.hash.0)).collect();
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
    // A coin spent by a transaction the server no longer lists: the block
    // that spent it is forgotten, and a payment of ours that spent it
    // still stands as sent, so it holds the coin again.
    let kept: HashSet<TxHash> = state.spends.iter().filter_map(|s| hash_bytes(&s.tx)).collect();
    let standing: HashSet<TxHash> = state.sent.iter().filter(|s| !s.released).filter_map(|s| hash_bytes(&s.tx)).collect();
    for i in 0..state.outputs.len() {
        let Coin::Spent { tx, height, .. } = state.outputs[i].coin else { continue };
        if kept.contains(&tx) {
            continue;
        }
        changed |= state.apply_at(i, Event::Rewind { from: height });
        if standing.contains(&tx) {
            state.apply_at(i, Event::SentByUs { tx });
        }
    }

    // Received, only at a subaddress the wallet watches, as the node's scan
    // finds them: the server holds the view key, so it can make an output
    // at any index, and Receive would move on past it.
    let plan = state.watch_plan();
    let mut known_keys: HashSet<String> = state.outputs.iter().map(|o| o.key.clone()).collect();
    for &(t, tx) in &mined {
        let height = tx.height.unwrap_or_default();
        let hash = hex::encode(tx.hash.0);
        let time = tx.timestamp.as_ref().and_then(|t| unix_time(&t.0)).unwrap_or(0);
        // As Monero writes it, and as the node's scan keeps it: a block, or
        // from TIME_LOCK_FROM on a Unix time.
        let unlock_block = tx.unlock_time;
        for (received, opened) in tx.receives.iter().zip(&opened.opened[t]) {
            let key = hex::encode(received.public_key.0);
            if known_keys.contains(&key) {
                continue;
            }
            let (account, subaddress) = (received.recipient.maj_i, received.recipient.min_i);
            let watched = plan.upto(account).is_some_and(|upto| subaddress <= upto);
            let Some(output) = watched.then(|| checked_output(tx, received, opened, unlock_block)).flatten() else {
                refused += 1;
                continue;
            };
            // Money to the split address doesn't move Receive on to it, nor
            // does money to another account.
            if account == 0 && Some(subaddress) != state.split_index {
                state.receive_index = state.receive_index.max(subaddress);
            }
            state.found_at(account, subaddress);
            let ki = match keys.key_image(output.key(), output.key_offset()) {
                Some(image) => KeyImage::Known(image),
                None => KeyImage::Unknown,
            };
            known_keys.insert(key.clone());
            state.outputs.push(Owned {
                output: hex::encode(output.serialize()),
                tx: hash.clone(),
                height,
                time,
                amount: received.amount.0,
                subaddress,
                account,
                coinbase: tx.coinbase,
                unlock_block,
                key,
                coin: Coin::Free { ki },
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
    let mut spends: HashMap<String, usize> = state.spends.iter().enumerate().rev().map(|(i, s)| (s.tx.clone(), i)).collect();
    let sent_at: HashMap<String, usize> = state.sent.iter().enumerate().rev().map(|(i, s)| (s.tx.clone(), i)).collect();
    for &(_, tx) in &mined {
        let height = tx.height.unwrap_or_default();
        let hash = hex::encode(tx.hash.0);
        let time = tx.timestamp.as_ref().and_then(|t| unix_time(&t.0)).unwrap_or(0);
        // 0 when the server doesn't say: no spend pays nothing, so a history
        // row reads it as not known.
        let fee = tx.fee.map_or(0, |f| f.0);
        for candidate in &tx.spent_outputs {
            let Some(&i) = images.get(&candidate.key_image.0) else { continue };
            match spends.get(&hash).map(|&s| &mut state.spends[s]) {
                None => {
                    spends.insert(hash.clone(), state.spends.len());
                    state.spends.push(ChainSpend { tx: hash.clone(), height, time, fee });
                    changed = true;
                }
                // Found before the server gave fees.
                Some(spend) if spend.fee == 0 && fee > 0 => {
                    spend.fee = fee;
                    changed = true;
                }
                Some(_) => {}
            }
            changed |= state.apply_at(i, Event::MinedSpend { tx: tx.hash.0, height });
        }
        if let Some(sent) = sent_at.get(&hash).map(|&s| &mut state.sent[s])
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

    // Not confirmed: a payment of this wallet's the server has neither
    // mined nor in its pool a day after it was sent, as the node path marks
    // one its node has never heard of. Mined later, it's taken back above.
    let pooled: HashSet<String> = answer.transactions.iter().filter(|t| t.mempool).map(|t| hex::encode(t.hash.0)).collect();
    for sent in &mut state.sent {
        if sent.height.is_none() && !sent.not_confirmed && !sent.released && now.saturating_sub(sent.time) >= NOT_CONFIRMED_AFTER && !pooled.contains(&sent.tx) {
            sent.not_confirmed = true;
            changed = true;
        }
    }

    // The pool.
    let mut pool: HashMap<String, Pooled> = HashMap::new();
    let ours: HashSet<&str> = state.sent.iter().map(|s| s.tx.as_str()).chain(state.outputs.iter().map(|o| o.tx.as_str())).collect();
    for (t, tx) in answer.transactions.iter().enumerate().filter(|(_, t)| t.mempool) {
        let hash = hex::encode(tx.hash.0);
        // A payment of this wallet's: its change isn't money coming in.
        if ours.contains(hash.as_str()) {
            continue;
        }
        for (received, opened) in tx.receives.iter().zip(&opened.opened[t]) {
            if !matches!(opened, Opened::Ours { .. }) {
                refused += 1;
                continue;
            }
            let seen = pool_before.get(&hash).map_or(now, |p| p.seen);
            pool.entry(hash.clone()).or_insert(Pooled { seen, ..Pooled::default() }).add(received.recipient.maj_i, received.recipient.min_i, received.amount.0);
        }
    }

    // A device's coins spent in the pool only: held by the pool transaction
    // that spends them, as the node path holds them, and let go once the
    // pool has forgotten it; mined, the spend above names it instead. A
    // device's key images are the ones it gave (`trezor_sync`), so this is
    // decided afresh at every poll. A wallet with its own spend key sees
    // its own payments' spends recorded already, as it sent them.
    if keys.device().is_some() {
        let images = Wallet::key_images(state);
        for tx in answer.transactions.iter().filter(|t| t.mempool) {
            for spend in &tx.spent_outputs {
                if let Some(&i) = images.get(&spend.key_image.0) {
                    changed |= state.apply_at(i, Event::PoolSpend { tx: tx.hash.0 });
                }
            }
        }
    }
    // A pool transaction's hold whose transaction the pool no longer has.
    let in_pool: HashSet<TxHash> = answer.transactions.iter().filter(|t| t.mempool).map(|t| t.hash.0).collect();
    for i in 0..state.outputs.len() {
        if let Coin::Held { hold: Hold::InPool { tx }, .. } = state.outputs[i].coin
            && !in_pool.contains(&tx)
        {
            changed |= state.apply_at(i, Event::PoolLeft { tx });
        }
    }

    // The clock.
    let latest = mined.iter().map(|(_, t)| t).filter(|t| vouched(t.height.unwrap_or_default())).filter_map(|t| t.timestamp.as_ref().and_then(|t| unix_time(&t.0))).filter(|&t| t <= now).max();
    if let Some(latest) = latest
        && latest > state.last_block_time
    {
        state.last_block_time = latest;
        changed = true;
    }

    if state.light_scanned_height != answer.scanned_height || state.tip != answer.blockchain_height {
        state.light_scanned_height = answer.scanned_height;
        state.tip = answer.blockchain_height;
        changed = true;
    }
    Ok(Reconciled { changed, pool, refused, checked, unchecked, proven, pending })
}

/// How this wallet's outputs in blocks were checked, in an answer [`vet`]
/// took: see [`Reconciled`].
#[derive(Debug, Default)]
pub(super) struct Tally {
    pub(super) checked: usize,
    pub(super) unchecked: usize,
    pub(super) proven: usize,
    pub(super) pending: usize,
}

/// Checks the whole answer before any of it is taken. Refuses it all when
/// a transaction's proof doesn't hold or names a block other than the one
/// `chain` (a node's word) has at its height ([`Problem::NotOnChain`]: the
/// server made up a payment, or is broken), when an amount doesn't open the
/// commitment the chain holds for that output, from the proof, or else the
/// one the server gives ([`Problem::AmountMismatch`]), or when it
/// contradicts itself ([`Problem::Contradiction`]): an output named twice
/// with different one-time keys, two outputs given one place on the chain,
/// two blocks at one height, or a transaction in a block well past the
/// chain's tip as the server gives it. Otherwise counts how this wallet's
/// outputs in blocks were checked. The proofs and outputs are as `checked`
/// found them.
pub(super) fn vet(answer: &lws::AddressTxs, checked: &Checked, chain: &Known) -> std::result::Result<Tally, Problem> {
    let mut tally = Tally::default();
    let mut places: HashMap<([u8; 32], u16), [u8; 32]> = HashMap::new();
    let mut global: HashMap<u64, [u8; 32]> = HashMap::new();
    let mut blocks: HashMap<u64, [u8; 32]> = HashMap::new();
    for (t, tx) in answer.transactions.iter().enumerate() {
        let mined = (!tx.mempool).then_some(tx.height).flatten();
        if mined.is_some_and(|h| h > answer.blockchain_height.saturating_add(TIP_SLACK)) {
            return Err(Problem::Contradiction);
        }
        // Its proof, if it has one and is in a block: it holds, and names
        // the block the node has there (`Some(true)`) or one no node has
        // vouched for yet (`Some(false)`).
        let proven = match &checked.proofs[t] {
            Some(Err(_)) => return Err(Problem::NotOnChain),
            Some(Ok(proven)) => proven.as_ref(),
            None => None,
        };
        let on_chain = match &proven {
            None => None,
            Some(p) => {
                if *blocks.entry(p.height).or_insert(p.block) != p.block {
                    return Err(Problem::Contradiction);
                }
                match chain.at(p.height) {
                    Some(block) if block == p.block => Some(true),
                    Some(_) => return Err(Problem::NotOnChain),
                    None => Some(false),
                }
            }
        };
        for (received, opened) in tx.receives.iter().zip(&checked.opened[t]) {
            let key = received.public_key.0;
            if *places.entry((tx.hash.0, received.index)).or_insert(key) != key {
                return Err(Problem::Contradiction);
            }
            if mined.is_some() && *global.entry(received.global_index).or_insert(key) != key {
                return Err(Problem::Contradiction);
            }
            let checked = match opened {
                Opened::Mismatch => return Err(Problem::AmountMismatch),
                Opened::NotOurs => continue,
                Opened::Ours { checked, .. } => *checked,
            };
            if mined.is_none() {
                // In the pool: not this wallet's until mined, and proved then.
                continue;
            }
            match (on_chain, checked) {
                (Some(true), _) => tally.proven += 1,
                (Some(false), _) => tally.pending += 1,
                (None, true) => tally.checked += 1,
                (None, false) => tally.unchecked += 1,
            }
        }
    }
    Ok(tally)
}

/// An answer as far as it can be checked without the wallet's state, for
/// [`vet`] and [`reconcile_checked`]: each transaction's proof (`None` in
/// the pool), and each output it lists received, [`opened`], with a proof
/// held to the chain's commitment. Worked out off the async runtime and
/// before the state lock is taken: it's most of a poll's work.
pub(super) struct Checked {
    pub(super) proofs: Vec<Option<std::result::Result<Option<proof::Proven>, proof::Broken>>>,
    pub(super) opened: Vec<Vec<Opened>>,
    /// The key images of this wallet's outputs in blocks listed (none for
    /// a device's): a transaction spending one is the wallet's too.
    pub(super) images: HashSet<[u8; 32]>,
}

/// Everything opening a received output depends on: the same again opens
/// the same.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct OpenKey {
    public_key: [u8; 32],
    tx_key: [u8; 32],
    index: u16,
    account: u32,
    subaddress: u32,
    rct: Option<[u8; 32]>,
    amount: u64,
    commitment: Option<[u8; 32]>,
    compact: bool,
    coinbase: bool,
}

/// Outputs a poll opened, with the key image of each of the wallet's, for
/// the next not to work them out again: only those its answer listed, so it
/// never grows past one answer.
#[derive(Default)]
pub(super) struct OpenCache(HashMap<OpenKey, (Opened, Option<[u8; 32]>)>);

/// [`Checked`] for `answer`, opening only what `last` (the last poll's)
/// doesn't have, with what this poll opened for the next.
pub(super) fn check(keys: &Keys, answer: &lws::AddressTxs, mut last: OpenCache) -> (Checked, OpenCache) {
    let mut kept = OpenCache::default();
    let mut images = HashSet::new();
    let mut proofs = Vec::with_capacity(answer.transactions.len());
    let mut opens = Vec::with_capacity(answer.transactions.len());
    for tx in &answer.transactions {
        let mined = (!tx.mempool).then_some(tx.height).flatten();
        let proof = mined.map(|_| proof::check(tx));
        let mut outputs = Vec::with_capacity(tx.receives.len());
        // One that doesn't hold refuses the answer: nothing to open.
        if let Some(Err(_)) = proof {
            proofs.push(proof);
            opens.push(outputs);
            continue;
        }
        let proven = proof.as_ref().and_then(|p| p.as_ref().ok()).and_then(Option::as_ref);
        for (i, received) in tx.receives.iter().enumerate() {
            // With a proof, the amount is held to the chain's commitment.
            let chain = proven.and_then(|p| p.commitments.get(i).copied().flatten());
            let commitment = chain.or(received.commitment.map(|c| c.0));
            let key = OpenKey {
                public_key: received.public_key.0,
                tx_key: received.tx_pub_key.0,
                index: received.index,
                account: received.recipient.maj_i,
                subaddress: received.recipient.min_i,
                rct: received.rct.map(|r| r.0),
                amount: received.amount.0,
                commitment,
                compact: mined.is_none_or(|h| h >= compact_from(keys.network)),
                coinbase: tx.coinbase,
            };
            let (output, image) = match last.0.remove(&key).or_else(|| kept.0.get(&key).cloned()) {
                Some(done) => done,
                None => {
                    let output = match chain {
                        Some(_) => opened(keys, tx, &lws::ReceivedOutput { commitment: commitment.map(lws::Hex), ..received.clone() }, mined),
                        None => opened(keys, tx, received, mined),
                    };
                    let image = match &output {
                        Opened::Ours { key_offset, .. } => monero_wallet::ed25519::CompressedPoint::from(received.public_key.0).decompress().and_then(|k| keys.key_image(k, *key_offset)),
                        _ => None,
                    };
                    (output, image)
                }
            };
            if mined.is_some() {
                images.extend(image);
            }
            kept.0.insert(key, (output.clone(), image));
            outputs.push(output);
        }
        proofs.push(proof);
        opens.push(outputs);
    }
    (Checked { proofs, opened: opens, images }, kept)
}

/// An output the server says this wallet received, as far as Fob can check it.
#[derive(Clone)]
pub(super) enum Opened {
    /// Its key isn't one this wallet's view key makes from the transaction
    /// key and index for the subaddress named, or its mask isn't the one it
    /// makes: not this wallet's.
    NotOurs,
    /// Its key and mask check, but the amount and mask don't open the
    /// commitment the server gives for it.
    Mismatch,
    /// This wallet's: its key offset and mask, and whether the amount was
    /// held to a commitment (`false`: the server gave none).
    Ours { key_offset: Scalar, mask: Scalar, checked: bool },
}

/// Checks an output the server says this wallet received, mined at
/// `height` (`None` in the pool): its one-time key is the one the view key
/// makes from the transaction key and index for the subaddress named (so a
/// server can't hand Fob an output under a key it doesn't own), its mask is
/// the one the view key makes (from hard fork 10 on; a mined reward's is 1;
/// before, it came encrypted and is the server's), and, when the server
/// gives the output's commitment, `mask·G + amount·H` is that commitment,
/// compared in constant time.
///
/// The commitment is the server's word for what the chain holds: checking
/// it binds the amount to it, so a server can't misstate an amount without
/// also misstating the commitment. Without a node to ask, Fob can't tell a
/// misstated commitment from the chain's; a coin with one can't be spent,
/// as the node refuses the transaction.
pub(super) fn opened(keys: &Keys, tx: &lws::Transaction, received: &lws::ReceivedOutput, height: Option<u64>) -> Opened {
    use subtle::ConstantTimeEq;
    let index = u64::from(received.index);
    let Some((key_offset, derived_mask)) = keys.light_output_at(&received.tx_pub_key.0, index, received.recipient.maj_i, received.recipient.min_i, &received.public_key.0) else {
        return Opened::NotOurs;
    };
    let compact = height.is_none_or(|h| h >= compact_from(keys.network));
    let said = match received.rct {
        Some(rct) => match Scalar::read(&mut rct.0.as_slice()) {
            Ok(said) => said,
            Err(_) => return Opened::NotOurs,
        },
        // A modern output's mask is the view key's to make, given or not.
        None if compact && !tx.coinbase => derived_mask,
        None => return Opened::NotOurs,
    };
    let mask = match tx.coinbase {
        true if said == monero_wallet::ed25519::Commitment::zero().mask => said,
        true => return Opened::NotOurs,
        false if said == derived_mask || !compact => said,
        false => return Opened::NotOurs,
    };
    let checked = match &received.commitment {
        None => false,
        Some(given) => {
            let made = monero_wallet::ed25519::Commitment::new(mask, received.amount.0).commit().compress().to_bytes();
            if !bool::from(made.ct_eq(&given.0)) {
                return Opened::Mismatch;
            }
            true
        }
    };
    Opened::Ours { key_offset, mask, checked }
}

/// The `WalletOutput` for an output the server says this wallet received,
/// once it checks (`opened`, by [`opened`]), locked as its transaction's
/// unlock time says: a block below `TIME_LOCK_FROM`, a Unix time from it on.
pub(super) fn checked_output(tx: &lws::Transaction, received: &lws::ReceivedOutput, opened: &Opened, unlock_block: u64) -> Option<WalletOutput> {
    let Opened::Ours { key_offset, mask, .. } = *opened else { return None };
    let index = u64::from(received.index);
    let payment_id = match tx.payment_id.as_ref().map(|p| p.0.as_slice()) {
        Some(id) if id.len() == 8 => Some(PaymentId::Encrypted(id.try_into().ok()?)),
        _ => None,
    };
    let timelock = match unlock_block {
        0 => Timelock::None,
        block if block < TIME_LOCK_FROM => Timelock::Block(usize::try_from(block).ok()?),
        time => Timelock::Time(time),
    };
    let bytes = wallet_output_bytes(&WalletOutputParts {
        tx: tx.hash.0,
        index,
        global_index: received.global_index,
        key: received.public_key.0,
        key_offset,
        mask,
        amount: received.amount.0,
        timelock,
        subaddress: SubaddressIndex::new(received.recipient.maj_i, received.recipient.min_i),
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
pub(super) use crate::light::proof::unix_time;
