//! Wallets whose spend key is on a Trezor, against a Trezor in software.

use super::*;

use curve25519_dalek::{EdwardsPoint, Scalar as DScalar, constants::ED25519_BASEPOINT_TABLE};
use monero_wallet::ed25519::Scalar;

fn scalar(hex_key: &str) -> DScalar {
    Scalar::read(&mut hex::decode(hex_key).unwrap().as_slice()).unwrap().into()
}

/// A block paying `amount` to `keys`' primary address at `height`, made by
/// hand: one transaction whose extra names a stray public key first and
/// the real one second, as some do.
fn block_paying(keys: &Keys, amount: u64, height: usize) -> (monero_wallet::interface::ScannableBlock, [u8; 32]) {
    use monero_wallet::{
        block::{Block, BlockHeader},
        ed25519::{Commitment, CompressedPoint},
        extra::ExtraField,
        ringct::{EncryptedAmount, PrunedRctProofs, RctBase, RctType},
        transaction::{Input, Output, Pruned, Timelock, Transaction, TransactionPrefix},
    };
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let r = DScalar::from_bytes_mod_order(bytes);
    let big_r = &r * ED25519_BASEPOINT_TABLE;
    OsRng.fill_bytes(&mut bytes);
    let stray = &DScalar::from_bytes_mod_order(bytes) * ED25519_BASEPOINT_TABLE;
    let address = MoneroAddress::from_str(keys.network.monero(), &keys.primary_address()).unwrap();
    let a = scalar(&keys.view_key_hex());
    let b: EdwardsPoint = address.spend().into();
    let derivation = (big_r * a).mul_by_cofactor().compress().to_bytes();
    let mut data = derivation.to_vec();
    data.push(0);
    let shared = Scalar::hash(&data);
    let shared_d: DScalar = shared.into();
    let view_tag = crate::trezor::crypto::keccak(&[b"view_tag".as_slice(), &data].concat())[0];
    let key = (&shared_d * ED25519_BASEPOINT_TABLE + b).compress().to_bytes();
    let mask = Scalar::hash([b"commitment_mask".as_slice(), &<[u8; 32]>::from(shared)].concat());
    let commitment = Commitment { mask, amount }.commit().compress();
    let amount_mask = crate::trezor::crypto::keccak(&[b"amount".as_slice(), &<[u8; 32]>::from(shared)].concat());
    let encrypted = (amount ^ u64::from_le_bytes(amount_mask[..8].try_into().unwrap())).to_le_bytes();
    let mut extra = ExtraField::PublicKey(CompressedPoint::from(stray.compress().to_bytes())).serialize();
    extra.extend(ExtraField::PublicKey(CompressedPoint::from(big_r.compress().to_bytes())).serialize());
    let tx: Transaction<Pruned> = Transaction::V2 {
        prefix: TransactionPrefix {
            additional_timelock: Timelock::None,
            inputs: vec![Input::ToKey { amount: None, key_offsets: vec![1; 16], key_image: CompressedPoint::from(stray.compress().to_bytes()) }],
            outputs: vec![Output { amount: None, key: CompressedPoint::from(key), view_tag: Some(view_tag) }],
            extra,
        },
        proofs: Some(PrunedRctProofs { rct_type: RctType::ClsagBulletproofPlus, base: RctBase { fee: 1000, pseudo_outs: vec![], encrypted_amounts: vec![EncryptedAmount::Compact { amount: encrypted }], commitments: vec![commitment] } }),
    };
    let hash = tx.hash_with_prunable_hash([0; 32]).unwrap();
    let miner = Transaction::V2 {
        prefix: TransactionPrefix {
            additional_timelock: Timelock::Block(height + 60),
            inputs: vec![Input::Gen(height)],
            outputs: vec![Output { amount: Some(1), key: CompressedPoint::from(stray.compress().to_bytes()), view_tag: Some(0) }],
            extra: ExtraField::PublicKey(CompressedPoint::from(stray.compress().to_bytes())).serialize(),
        },
        proofs: None,
    };
    let block = Block::new(BlockHeader { hardfork_version: 16, hardfork_signal: 16, timestamp: 1_700_000_000, previous: [0; 32], nonce: 0 }, miner, vec![hash]).unwrap();
    (monero_wallet::interface::ScannableBlock { block, transactions: vec![tx], output_index_for_first_ringct_output: Some(0) }, big_r.compress().to_bytes())
}

/// A Trezor in software holding `full`'s keys, answering as the device
/// would: its view key, key images made with the spend key (for the main
/// address or a subaddress, from the transaction key given), and payments
/// signed with it.
pub(super) fn fake_trezor(full: &Keys) -> crate::trezor::session::fake::Fake {
    let mut fake = crate::trezor::session::fake::Fake::new();
    fake.address = full.primary_address();
    fake.view_key = scalar(&full.view_key_hex()).to_bytes();
    let spend = scalar(&full.spend_key_hex().unwrap());
    let view = scalar(&full.view_key_hex());
    fake.signer = Some(crate::trezor::fake_signer::Signer::new(spend, view));
    fake.key_image = Box::new(move |t: &crate::trezor::Transfer| {
        // The output's secret key from its transaction key, as the device makes it.
        let big_r = curve25519_dalek::edwards::CompressedEdwardsY(t.tx_pub_key).decompress().unwrap();
        let mut data = (big_r * view).mul_by_cofactor().compress().to_bytes().to_vec();
        data.extend_from_slice(&crate::trezor::proto::varint(t.index_in_tx));
        let sub: DScalar = match t.subaddress_minor {
            0 => DScalar::ZERO,
            minor => Scalar::hash([b"SubAddr\0".as_slice(), view.as_bytes(), &0u32.to_le_bytes(), &minor.to_le_bytes()].concat()).into(),
        };
        let x: DScalar = Scalar::hash(&data).into() + spend + sub;
        assert_eq!((&x * ED25519_BASEPOINT_TABLE).compress().to_bytes(), t.out_key, "the transaction key given is the output's");
        let hashed: EdwardsPoint = monero_wallet::ed25519::Point::biased_hash(t.out_key).into();
        let image = (hashed * x).compress().to_bytes();
        let (c, r) = crate::trezor::crypto::sign_key_image(&image, &x, &t.out_key);
        (image, c, r)
    });
    fake
}

/// A wallet on a Trezor: the device gives the view key, the wallet opens
/// again as a device's, and payments wait for the device.
#[tokio::test]
async fn a_trezor_wallet_keeps_its_keys_on_the_device() {
    let full = Keys::generate(Network::Stagenet).keys;
    let fake = fake_trezor(&full);
    let person = crate::trezor::session::fake::Person::new();
    let path = dir("trezor");
    let wallet = Wallet::restore_trezor(&path, [1; 32], Network::Stagenet, &fake, &person, Some(5)).unwrap();
    assert!(wallet.can_spend() && wallet.is_trezor());
    assert_eq!(wallet.primary_address(), full.primary_address());
    assert_eq!(wallet.inner.keys.device(), Some(Device::Trezor));
    assert!(wallet.inner.keys.spend_key_hex().is_none());
    let hub = wallet.backup_hub(true).await;
    assert!(hub.backup.iter().all(|r| r.item != backup::HubItem::SeedPhrase), "no seed phrase to show");
    assert!(hub.footer.contains("on your Trezor"));
    assert!(wallet.backup_reminder().await.is_none(), "nothing to write down");
    drop(wallet);
    let again = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(again.inner.keys.device(), Some(Device::Trezor));
    assert!(again.can_spend());
    assert!(matches!(again.send(0).await, Err(FobError::TrezorNeeded)));
    assert_eq!(again.trezor_unsynced().await, 0);
}

/// A cable that swaps the exported address is caught: the device shows the
/// real one on its own screen and says it again, and the two must agree.
#[tokio::test]
async fn a_trezor_whose_shown_address_differs_makes_no_wallet() {
    let full = Keys::generate(Network::Stagenet).keys;
    let mut fake = fake_trezor(&full);
    fake.shown_address = Some(Keys::generate(Network::Stagenet).keys.primary_address());
    let person = crate::trezor::session::fake::Person::new();
    let path = dir("trezor-swapped");
    assert!(matches!(Wallet::restore_trezor(&path, [1; 32], Network::Stagenet, &fake, &person, Some(5)), Err(FobError::Trezor(t)) if t.contains("isn't the one it sent")));
    assert!(!path.with_extension("db").exists() || Wallet::open(&path, [1; 32]).is_err(), "nothing kept");
}

/// Money found for a Trezor wallet keeps the key its transaction was made
/// with (the right one of several), can't be spent until the device has
/// given its key image, and the node is asked whether that image is spent
/// already.
#[tokio::test]
async fn a_trezor_wallet_s_outputs_wait_for_their_key_images() {
    let full = Keys::generate(Network::Stagenet).keys;
    let fake = fake_trezor(&full);
    let person = crate::trezor::session::fake::Person::new();
    let (url, _) = counting_node(Duration::ZERO).await;
    let wallet = Wallet::restore_trezor(&dir("trezor-sync"), [1; 32], Network::Stagenet, &fake, &person, Some(5)).unwrap();
    wallet.inner.state.lock().await.node = url;

    let (block, tx_key) = block_paying(&wallet.inner.keys, 7_000_000_000, 10);
    let hashes = [block.block.hash()];
    let (scanner, prefilter) = wallet.scanner(10);
    let (results, _) = scan::scan(&scanner, &prefilter, vec![block], &hashes, false);
    let scanned = results.into_iter().next().unwrap().unwrap();
    assert_eq!(scanned.found.len(), 1);
    {
        let mut state = wallet.inner.state.lock().await;
        let mut images = Wallet::key_images(&state);
        assert!(wallet.take_block(&mut state, &mut images, &HashMap::new(), scanned));
        state.tip = 100;
        assert_eq!(state.outputs.len(), 1);
        assert_eq!(state.outputs[0].tx_key.as_deref(), Some(hex::encode(tx_key).as_str()), "the real key, not the stray one");
        assert_eq!(state.outputs[0].extra_key, None);
        assert_eq!(state.outputs[0].key_image, None, "no spend key here to make it");
        assert_eq!(state.outputs[0].amount, 7_000_000_000);
    }
    assert_eq!(wallet.trezor_unsynced().await, 1);
    // Not spendable until the device has been asked.
    match wallet.prepare(&full.address(3), 1_000_000, None, None).await {
        Err(FobError::TrezorNeeded) => {}
        other => panic!("expected the device to be asked for, got {other:?}"),
    }

    let synced = wallet.trezor_sync(Arc::new(fake), Arc::new(person)).await.unwrap();
    assert_eq!(synced, 1);
    assert_eq!(wallet.trezor_unsynced().await, 0);
    let state = wallet.inner.state.lock().await;
    let image = state.outputs[0].key_image.clone().unwrap();
    let output = WalletOutput::read(&mut hex::decode(&state.outputs[0].output).unwrap().as_slice()).unwrap();
    assert_eq!(image, hex::encode(full.key_image(output.key(), output.key_offset()).unwrap()), "the device's key image is the one the spend key gives");
    // The counting node says every key image is spent: so it is marked, with no transaction to name.
    assert_eq!(state.outputs[0].spent_in.as_deref(), Some(SPENT_BEFORE_SYNC));
    drop(state);

    // A rewind (a reorg, a rescan) keeps what the device's sync found; a
    // spend by a transaction that fell out it would not.
    {
        let mut state = wallet.inner.state.lock().await;
        let copy = state.outputs[0].clone();
        state.outputs.push(Owned { spent_in: Some("deadbeef".into()), height: 5, ..copy });
    }
    wallet.rewind(50, None).await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.outputs[0].spent_in.as_deref(), Some(SPENT_BEFORE_SYNC), "the mark stays");
    assert_eq!(state.outputs[1].spent_in, None, "an unknown transaction's spend is let go");
    drop(state);

    // Asked again with the device already done, the node is asked again and nothing else is.
    let before = wallet.inner.state.lock().await.outputs[0].key_image.clone();
    let fake2 = fake_trezor(&full);
    let person2 = crate::trezor::session::fake::Person::new();
    assert_eq!(wallet.trezor_sync(Arc::new(fake2), Arc::new(person2)).await.unwrap(), 0);
    assert_eq!(wallet.inner.state.lock().await.outputs[0].key_image, before);
}

/// Spent in the pool only is a tentative mark: the next sync asks the node
/// again, and a spend that never got mined frees the coin.
#[tokio::test]
async fn a_pool_spend_from_the_node_is_asked_about_again() {
    let full = Keys::generate(Network::Stagenet).keys;
    let fake = fake_trezor(&full);
    let person = crate::trezor::session::fake::Person::new();
    let answer = Arc::new(std::sync::Mutex::new("2".to_owned()));
    let answers = answer.clone();
    let url = scripted_node(move |text| {
        let answers = answers.clone();
        async move {
            if text.contains("is_key_image_spent") {
                format!(r#"{{"spent_status":[{}],"status":"OK"}}"#, answers.lock().unwrap())
            } else if text.contains("get_height") {
                r#"{"height":3000000,"status":"OK"}"#.to_owned()
            } else {
                r#"{"jsonrpc":"2.0","id":0,"result":{}}"#.to_owned()
            }
        }
    })
    .await;
    let wallet = Wallet::restore_trezor(&dir("trezor-pool"), [1; 32], Network::Stagenet, &fake, &person, Some(5)).unwrap();
    wallet.inner.state.lock().await.node = url;
    let (block, _) = block_paying(&wallet.inner.keys, 5_000_000_000, 10);
    let hashes = [block.block.hash()];
    let (scanner, prefilter) = wallet.scanner(10);
    let (results, _) = scan::scan(&scanner, &prefilter, vec![block], &hashes, false);
    {
        let mut state = wallet.inner.state.lock().await;
        let mut images = Wallet::key_images(&state);
        wallet.take_block(&mut state, &mut images, &HashMap::new(), results.into_iter().next().unwrap().unwrap());
        state.tip = 100;
    }
    wallet.trezor_sync(Arc::new(fake), Arc::new(person)).await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs[0].spent_in.as_deref(), Some(SPENT_IN_POOL_AT_SYNC));
    // The pool forgot it: the coin is free again.
    *answer.lock().unwrap() = "0".into();
    let (fake, person) = (fake_trezor(&full), crate::trezor::session::fake::Person::new());
    wallet.trezor_sync(Arc::new(fake), Arc::new(person)).await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs[0].spent_in, None);
    // Mined after all: spent for good.
    *answer.lock().unwrap() = "1".into();
    let (fake, person) = (fake_trezor(&full), crate::trezor::session::fake::Person::new());
    wallet.trezor_sync(Arc::new(fake), Arc::new(person)).await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs[0].spent_in.as_deref(), Some(SPENT_BEFORE_SYNC));
}
