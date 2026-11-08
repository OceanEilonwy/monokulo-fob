//! A wallet whose spend key is on a Trezor: the key images it gives, and
//! the payments it signs. The protocol itself is [`crate::trezor`].

use super::*;
use coin::{Coin, Event, Said};

/// What's waiting on a Trezor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrezorWaiting {
    /// Outputs the device hasn't given the key image of: a sync does it.
    pub unsynced: u32,
    /// Outputs kept before Fob recorded transaction keys, which the device
    /// can't be asked about: a rescan from before them puts that right.
    pub frozen: u32,
}

impl Wallet {
    /// What's waiting on this wallet's Trezor: nothing, for other wallets.
    pub async fn trezor_waiting(&self) -> TrezorWaiting {
        if self.inner.keys.device() != Some(Device::Trezor) {
            return TrezorWaiting::default();
        }
        let state = self.inner.state.lock().await;
        let open = state.outputs.iter().filter(|o| o.coin.is_free() && o.coin.key_image().is_none());
        let (frozen, unsynced) = open.fold((0, 0), |(frozen, unsynced), o| if o.tx_key.is_some() { (frozen, unsynced + 1) } else { (frozen + 1, unsynced) });
        TrezorWaiting { unsynced, frozen }
    }

    /// How many of this wallet's outputs its Trezor hasn't been asked the
    /// key image of: until it has, they can't be spent, and a spend of
    /// them elsewhere goes unseen.
    pub async fn trezor_unsynced(&self) -> u32 {
        self.trezor_waiting().await.unsynced
    }

    /// Asks the Trezor over `link` for the key images it hasn't given yet,
    /// keeps them, then asks the node which of this wallet's key images are
    /// spent (all of them, each time: an answer from before may have been
    /// about the pool, or wrong). Through a light wallet server, the server
    /// is polled instead: every spend it lists, mined or in its pool, is
    /// held to the key images now known, as each poll does (`reconcile`),
    /// and no key image leaves the phone. Returns how many outputs the
    /// device covered. The device asks before it answers.
    pub async fn trezor_sync(&self, link: Arc<dyn trezor::Link>, prompts: Arc<dyn trezor::Prompts>) -> Result<u32> {
        if self.inner.keys.device() != Some(Device::Trezor) {
            return Err(FobError::Send("this wallet's keys aren't on a Trezor".into()));
        }
        let network = self.network();
        let (ids, transfers, minors, frozen) = {
            let mut state = self.inner.state.lock().await;
            // The sync decides every coin afresh: what a rewind remembered
            // of dropped coins is moot.
            state.remembered.clear();
            let mut ids = vec![];
            let mut transfers = vec![];
            let mut minors = vec![0];
            let mut frozen = 0;
            for o in &state.outputs {
                if o.coin.key_image().is_some() {
                    continue;
                }
                let key = |hex_key: &str| hex::decode(hex_key).ok().and_then(|k| <[u8; 32]>::try_from(k).ok());
                let (Some(tx_key), Some(out_key)) = (o.tx_key.as_deref().and_then(key), key(&o.key)) else {
                    frozen += 1;
                    continue;
                };
                let Some(output) = hex::decode(&o.output).ok().and_then(|b| WalletOutput::read(&mut b.as_slice()).ok()) else { continue };
                ids.push(o.id());
                transfers.push(trezor::Transfer {
                    out_key,
                    tx_pub_key: tx_key,
                    additional: o.extra_key.as_deref().and_then(key),
                    index_in_tx: output.index_in_transaction(),
                    subaddress_minor: o.subaddress,
                });
                minors.push(o.subaddress);
            }
            minors.sort_unstable();
            minors.dedup();
            (ids, transfers, minors, frozen)
        };
        let asked = transfers.len();
        if asked > 0 {
            let images = tokio::task::spawn_blocking(move || {
                let session = trezor::Session::open(link.as_ref(), prompts.as_ref())?;
                session.key_images(network, &transfers, &minors)
            })
            .await
            .map_err(|e| FobError::Send(format!("{e}")))??;
            // Kept at once: the device needn't be asked again if the node
            // (or the server) is away.
            let mut state = self.inner.state.lock().await;
            for (id, image) in ids.iter().zip(images) {
                state.apply(id, Event::LearnedKeyImage(image));
            }
            self.save_state(&state)?;
        }
        if self.light_mode().await {
            // The server's spends, matched now rather than at the next poll.
            self.light_step().await?;
        } else {
            self.mark_spent_by_node().await?;
        }
        if asked == 0 && frozen > 0 {
            return Err(FobError::Trezor(fob_words::errors::trezor_frozen(frozen)));
        }
        Ok(ids.len() as u32)
    }

    /// Asks the node which of this wallet's key images are spent, every coin
    /// not already spent on the chain, and tells each coin what it said:
    /// spent on the chain holds it until a sync says otherwise; spent in
    /// the pool only is tentative, asked again next time; unspent frees a
    /// coin a device hold held (decision 364). What `step` makes of each
    /// answer in each state is the table in `coin.rs`.
    async fn mark_spent_by_node(&self) -> Result<()> {
        let (ids, images): (Vec<CoinId>, Vec<String>) = {
            let state = self.inner.state.lock().await;
            state
                .outputs
                .iter()
                .filter(|o| !matches!(o.coin, Coin::Spent { .. }))
                .filter_map(|o| Some((o.id(), o.key_image_hex()?)))
                .unzip()
        };
        if images.is_empty() {
            return Ok(());
        }
        let spent = self.key_images_spent(&images).await?;
        let mut state = self.inner.state.lock().await;
        let at = state.tip;
        let mut changed = false;
        for (id, status) in ids.iter().zip(spent) {
            let answer = match status {
                0 => Said::Unspent,
                1 => Said::OnChain,
                _ => Said::InPool,
            };
            changed |= state.apply(id, Event::DeviceSaid { answer, at });
        }
        if changed {
            self.save_state(&state)?;
        }
        Ok(())
    }

    /// The node's word on `images`: 0 unspent, 1 spent on the chain, 2 in
    /// the pool.
    async fn key_images_spent(&self, images: &[String]) -> Result<Vec<u8>> {
        let (_, rpc, _, _) = self.connection().await?;
        let body = serde_json::json!({ "key_images": images }).to_string();
        let response = rpc.rpc_call("is_key_image_spent", Some(body), 1024 * 1024).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        let value: serde_json::Value = serde_json::from_str(&response).map_err(|e| FobError::Node(format!("reading the node's answer: {e}")))?;
        if value.get("status").and_then(|s| s.as_str()) != Some("OK") {
            return Err(FobError::Node("the node couldn't say which key images are spent".into()));
        }
        let statuses = value.get("spent_status").and_then(|s| s.as_array()).ok_or_else(|| FobError::Node("the node didn't say which key images are spent".into()))?;
        if statuses.len() != images.len() {
            return Err(FobError::Node("the node answered for the wrong number of key images".into()));
        }
        Ok(statuses.iter().map(|s| s.as_u64().unwrap_or(0) as u8).collect())
    }

    /// Signs a prepared payment on the Trezor over `link`, the device's
    /// questions put through `prompts`, and broadcasts it through the node,
    /// or the light wallet server syncing is through. Returns its
    /// transaction hash. The device shows the payment and asks before it
    /// signs; saying no there cancels, with the payment kept prepared. A
    /// payment that times out while the device is being waited on is still
    /// sent once signed: what's recorded of it was taken before.
    pub async fn send_with_trezor(&self, id: u64, link: Arc<dyn trezor::Link>, prompts: Arc<dyn trezor::Prompts>) -> Result<String> {
        let (plan, out) = {
            let mut prepared = self.inner.prepared.lock().await;
            let Some(prepared) = prepared.get(id) else { return Err(FobError::Screen(ScreenError::NoSuchPayment)) };
            (prepared.plan.as_ref().ok_or_else(|| FobError::Send("this payment wasn't prepared for a Trezor".into()))?.clone(), prepared.out.clone())
        };
        let fee = plan.fee;
        let view = self.inner.keys.view_bytes();
        let tx = tokio::task::spawn_blocking(move || {
            let session = trezor::Session::open(link.as_ref(), prompts.as_ref())?;
            trezor::sign::sign(&session, &plan, &view, &mut OsRng)
        })
        .await
        .map_err(|e| FobError::Send(format!("{e}")))??;
        // Signed: it's going out, prepared still or not.
        self.inner.prepared.lock().await.take(id);
        let hash = tx.hash();
        let signed = hex::encode(tx.serialize());
        let publish = self.publishing(tx).await;
        self.publish_and_record(hash, fee, out, signed, publish).await
    }
}
