//! A wallet whose spend key is on a Trezor: the key images it gives, and
//! the payments it signs. The protocol itself is [`crate::trezor`].

use super::*;

impl Wallet {
    /// How many of this wallet's outputs its Trezor hasn't been asked the
    /// key image of: until it has, they can't be spent, and a spend of
    /// them elsewhere goes unseen.
    pub async fn trezor_unsynced(&self) -> u32 {
        if self.inner.keys.device() != Some(Device::Trezor) {
            return 0;
        }
        let state = self.inner.state.lock().await;
        state.outputs.iter().filter(|o| o.spent_in.is_none() && o.key_image.is_none() && o.tx_key.is_some()).count() as u32
    }

    /// Asks the Trezor over `link` for the key images it hasn't given yet,
    /// then the node which of those are spent already. Returns how many
    /// outputs it covered. The device asks before it answers.
    pub async fn trezor_sync(&self, link: Arc<dyn trezor::Link>, prompts: Arc<dyn trezor::Prompts>) -> Result<u32> {
        if self.inner.keys.device() != Some(Device::Trezor) {
            return Err(FobError::Send("this wallet's keys aren't on a Trezor".into()));
        }
        let network = self.network();
        let (ids, transfers, minors) = {
            let state = self.inner.state.lock().await;
            let mut ids = vec![];
            let mut transfers = vec![];
            let mut minors = vec![0];
            for o in &state.outputs {
                if o.spent_in.is_some() || o.key_image.is_some() {
                    continue;
                }
                let key = |hex_key: &str| hex::decode(hex_key).ok().and_then(|k| <[u8; 32]>::try_from(k).ok());
                let (Some(tx_key), Some(out_key)) = (o.tx_key.as_deref().and_then(key), key(&o.key)) else { continue };
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
            (ids, transfers, minors)
        };
        if transfers.is_empty() {
            return Ok(0);
        }
        let images = tokio::task::spawn_blocking(move || {
            let session = trezor::Session::open(link.as_ref(), prompts.as_ref())?;
            session.key_images(network, &transfers, &minors)
        })
        .await
        .map_err(|e| FobError::Send(format!("{e}")))??;
        let images: Vec<String> = images.iter().map(hex::encode).collect();
        let spent = self.key_images_spent(&images).await?;
        let count = {
            let mut state = self.inner.state.lock().await;
            for ((id, image), status) in ids.iter().zip(images).zip(spent) {
                let Some(output) = state.outputs.iter_mut().find(|o| o.id() == *id) else { continue };
                output.key_image = Some(image);
                if status != 0 && output.spent_in.is_none() {
                    output.spent_in = Some(SPENT_BEFORE_SYNC.into());
                }
            }
            self.save_state(&state)?;
            ids.len() as u32
        };
        Ok(count)
    }

    /// The node's word on `images`: 0 unspent, 1 spent on the chain, 2 in
    /// the pool.
    async fn key_images_spent(&self, images: &[String]) -> Result<Vec<u8>> {
        let (_, rpc, _, _) = self.connection().await?;
        let body = serde_json::json!({ "key_images": images }).to_string();
        let response = rpc.rpc_call("is_key_image_spent", Some(body), 1024 * 1024).await.map_err(|e| FobError::Node(format!("{e:?}")))?;
        let value: serde_json::Value = serde_json::from_str(&response).map_err(|e| FobError::Node(format!("reading the node's answer: {e}")))?;
        let statuses = value.get("spent_status").and_then(|s| s.as_array()).ok_or_else(|| FobError::Node("the node didn't say which key images are spent".into()))?;
        if statuses.len() != images.len() {
            return Err(FobError::Node("the node answered for the wrong number of key images".into()));
        }
        Ok(statuses.iter().map(|s| s.as_u64().unwrap_or(0) as u8).collect())
    }

    /// Signs a prepared payment on the Trezor over `link`, the device's
    /// questions put through `prompts`, and broadcasts it. Returns its
    /// transaction hash. The device shows the payment and asks before it
    /// signs; saying no there cancels, with the payment kept prepared.
    pub async fn send_with_trezor(&self, id: u64, link: Arc<dyn trezor::Link>, prompts: Arc<dyn trezor::Prompts>) -> Result<String> {
        let plan = {
            let mut prepared = self.inner.prepared.lock().await;
            let Some(prepared) = prepared.get(id) else { return Err(FobError::Screen(ScreenError::NoSuchPayment)) };
            prepared.plan.as_ref().ok_or_else(|| FobError::Send("this payment wasn't prepared for a Trezor".into()))?.clone()
        };
        let fee = plan.fee;
        let tx = tokio::task::spawn_blocking(move || {
            let session = trezor::Session::open(link.as_ref(), prompts.as_ref())?;
            trezor::sign::sign(&session, &plan, &mut OsRng)
        })
        .await
        .map_err(|e| FobError::Send(format!("{e}")))??;
        let prepared = self.inner.prepared.lock().await.take(id).ok_or(FobError::Screen(ScreenError::NoSuchPayment))?;
        let hash = hex::encode(tx.hash());
        let signed = hex::encode(tx.serialize());
        let wallet = self.clone();
        let manual_sync = self.inner.state.lock().await.manual_sync;
        let publish = async move {
            let rpc = wallet.rpc().await.map_err(Unpublished::Refused)?;
            rpc.publish_transaction(&tx).await.map_err(|e| publish_failure(e, manual_sync))
        };
        self.publish_and_record(hash, fee, prepared.out, signed, publish).await
    }
}
