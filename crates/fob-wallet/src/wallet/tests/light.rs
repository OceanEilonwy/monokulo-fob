//! Syncing through a light wallet server: `reconcile`, the `WalletOutput`
//! bytes, the migration, the setting, and the light path against the node
//! path on the same blocks.

use super::*;

use crate::light::lws;
use curve25519_dalek::{EdwardsPoint, Scalar as DScalar, constants::ED25519_BASEPOINT_TABLE};
use crate::wallet::light::{WalletOutputParts, reconcile, reconcile_proven, unix_time, wallet_output_bytes};
use crate::light::{hashes::{self, Known}, proof};
use crate::light::Problem;
use monero_wallet::{
    block::{Block, BlockHeader},
    ed25519::{Commitment, CompressedPoint, Scalar},
    extra::ExtraField,
    interface::ScannableBlock,
    ringct::{EncryptedAmount, PrunedRctProofs, RctBase},
    transaction::{Input, Output, Pruned, Transaction, TransactionPrefix},
};

fn random_scalar() -> DScalar {
    let mut bytes = [0u8; 64];
    OsRng.fill_bytes(&mut bytes);
    DScalar::from_bytes_mod_order_wide(&bytes)
}

fn view_scalar(keys: &Keys) -> DScalar {
    Scalar::read(&mut hex::decode(keys.view_key_hex().as_str()).unwrap().as_slice()).unwrap().into()
}

/// An output paid to this wallet, as the chain and a server know it.
#[derive(Clone, Debug)]
struct Paid {
    index: u16,
    key: [u8; 32],
    /// The transaction key it was derived from.
    tx_key: [u8; 32],
    mask: [u8; 32],
    amount: u64,
    subaddress: u32,
    account: u32,
}

/// An output paying `amount` to `keys`' subaddress `sub` (0: the main
/// address) at `index`, from a fresh transaction key: the key, the view tag,
/// the transaction key, the mask, the encrypted amount.
fn pay(keys: &Keys, amount: u64, sub: u32, index: u16, coinbase: bool) -> (Output, Paid, Option<(Commitment, [u8; 8])>) {
    pay_at(keys, amount, 0, sub, index, coinbase)
}

/// [`pay`] to subaddress `sub` of `account`.
fn pay_at(keys: &Keys, amount: u64, account: u32, sub: u32, index: u16, coinbase: bool) -> (Output, Paid, Option<(Commitment, [u8; 8])>) {
    let spend: EdwardsPoint = match SubaddressIndex::new(account, sub) {
        Some(s) => keys.view_pair().subaddress(keys.network.monero(), s).spend().into(),
        None => keys.view_pair().spend().into(),
    };
    let r = random_scalar();
    // To a subaddress the transaction key is r·D, to the main address r·G.
    let big_r = if (account, sub) == (0, 0) { &r * ED25519_BASEPOINT_TABLE } else { spend * r };
    let mut data = (big_r * view_scalar(keys)).mul_by_cofactor().compress().to_bytes().to_vec();
    data.push(index as u8);
    let shared = Scalar::hash(&data);
    let shared_d: DScalar = shared.into();
    let view_tag = crate::trezor::crypto::keccak(&[b"view_tag".as_slice(), &data].concat())[0];
    let key = (&shared_d * ED25519_BASEPOINT_TABLE + spend).compress().to_bytes();
    let (mask, encrypted) = if coinbase {
        (Commitment::zero().mask, None)
    } else {
        let mask = Scalar::hash([b"commitment_mask".as_slice(), &<[u8; 32]>::from(shared)].concat());
        let amount_mask = crate::trezor::crypto::keccak(&[b"amount".as_slice(), &<[u8; 32]>::from(shared)].concat());
        let encrypted = (amount ^ u64::from_le_bytes(amount_mask[..8].try_into().unwrap())).to_le_bytes();
        (mask, Some((Commitment { mask, amount }, encrypted)))
    };
    let output = Output { amount: coinbase.then_some(amount), key: CompressedPoint::from(key), view_tag: Some(view_tag) };
    let paid = Paid { index, key, tx_key: big_r.compress().to_bytes(), mask: <[u8; 32]>::from(mask), amount, subaddress: sub, account };
    (output, paid, encrypted)
}

fn stray_point() -> CompressedPoint {
    CompressedPoint::from((&random_scalar() * ED25519_BASEPOINT_TABLE).compress().to_bytes())
}

/// A transaction paying `amount` to `sub` (or nobody, with `None`), spending
/// `images`, with a fee of 1,000.
fn transaction(keys: &Keys, pays: Option<(u64, u32)>, images: &[[u8; 32]]) -> (Transaction<Pruned>, [u8; 32], Option<Paid>) {
    locked_transaction(keys, pays, images, Timelock::None)
}

/// [`transaction`], locked as `timelock` says.
fn locked_transaction(keys: &Keys, pays: Option<(u64, u32)>, images: &[[u8; 32]], timelock: Timelock) -> (Transaction<Pruned>, [u8; 32], Option<Paid>) {
    transaction_at(keys, pays.map(|(amount, sub)| (amount, 0, sub)), images, timelock)
}

/// [`locked_transaction`] paying (amount, account, index).
fn transaction_at(keys: &Keys, pays: Option<(u64, u32, u32)>, images: &[[u8; 32]], timelock: Timelock) -> (Transaction<Pruned>, [u8; 32], Option<Paid>) {
    let (output, paid, commitment, encrypted, tx_key) = match pays {
        Some((amount, account, sub)) => {
            let (output, paid, enc) = pay_at(keys, amount, account, sub, 0, false);
            let (commitment, encrypted) = enc.unwrap();
            let tx_key = paid.tx_key;
            (output, Some(paid), commitment.commit().compress(), encrypted, tx_key)
        }
        None => {
            let key = stray_point();
            (Output { amount: None, key, view_tag: Some(0) }, None, Commitment { mask: Scalar::hash(b"x"), amount: 1 }.commit().compress(), [0; 8], stray_point().to_bytes())
        }
    };
    let mut inputs: Vec<Input> = images.iter().map(|i| Input::ToKey { amount: None, key_offsets: vec![1; 16], key_image: CompressedPoint::from(*i) }).collect();
    if inputs.is_empty() {
        inputs.push(Input::ToKey { amount: None, key_offsets: vec![1; 16], key_image: stray_point() });
    }
    let tx: Transaction<Pruned> = Transaction::V2 {
        prefix: TransactionPrefix { additional_timelock: timelock, inputs, outputs: vec![output], extra: ExtraField::PublicKey(CompressedPoint::from(tx_key)).serialize() },
        proofs: Some(PrunedRctProofs {
            rct_type: RctType::ClsagBulletproofPlus,
            base: RctBase { fee: 1000, pseudo_outs: vec![], encrypted_amounts: vec![EncryptedAmount::Compact { amount: encrypted }], commitments: vec![commitment] },
        }),
    };
    let hash = tx.hash_with_prunable_hash([0; 32]).unwrap();
    (tx, hash, paid)
}

/// A mined block at `height` whose reward pays `reward` to the main address
/// (or nobody), holding `txs`.
fn block(keys: &Keys, height: usize, reward: Option<u64>, txs: Vec<(Transaction<Pruned>, [u8; 32])>) -> (ScannableBlock, [u8; 32], Option<Paid>) {
    let (output, paid, tx_key) = match reward {
        Some(amount) => {
            let (output, paid, _) = pay(keys, amount, 0, 0, true);
            let tx_key = paid.tx_key;
            (output, Some(paid), tx_key)
        }
        None => (Output { amount: Some(1), key: stray_point(), view_tag: Some(0) }, None, stray_point().to_bytes()),
    };
    let miner = Transaction::V2 {
        prefix: TransactionPrefix { additional_timelock: Timelock::Block(height + 60), inputs: vec![Input::Gen(height)], outputs: vec![output], extra: ExtraField::PublicKey(CompressedPoint::from(tx_key)).serialize() },
        proofs: None,
    };
    let hashes = txs.iter().map(|t| t.1).collect();
    let block = Block::new(BlockHeader { hardfork_version: 16, hardfork_signal: 16, timestamp: 1_700_000_000 + height as u64 * 120, previous: [0; 32], nonce: 0 }, miner, hashes).unwrap();
    let miner_hash = block.miner_transaction().hash();
    let scannable = ScannableBlock { block, transactions: txs.into_iter().map(|t| t.0).collect(), output_index_for_first_ringct_output: Some(height as u64 * 10) };
    (scannable, miner_hash, paid)
}

fn received(paid: &Paid, global_index: u64) -> lws::ReceivedOutput {
    lws::ReceivedOutput {
        amount: lws::Amount(paid.amount),
        public_key: lws::Hex(paid.key),
        index: paid.index,
        global_index,
        tx_pub_key: lws::Hex(paid.tx_key),
        rct: Some(lws::Hex(paid.mask)),
        commitment: Some(lws::Hex(commitment_of(paid))),
        recipient: lws::AddressMeta { maj_i: paid.account, min_i: paid.subaddress },
    }
}

/// The commitment the chain holds for `paid`: `mask·G + amount·H`.
fn commitment_of(paid: &Paid) -> [u8; 32] {
    let mask = Scalar::read(&mut paid.mask.as_slice()).unwrap();
    Commitment::new(mask, paid.amount).commit().compress().to_bytes()
}

fn listed(hash: [u8; 32], height: Option<u64>, receives: Vec<lws::ReceivedOutput>, spent_outputs: Vec<lws::Spend>) -> lws::Transaction {
    lws::Transaction {
        id: 1,
        hash: lws::Hex(hash),
        timestamp: height.map(|h| lws::Timestamp::from_unix(1_700_000_000 + h * 120)),
        total_received: lws::Amount(receives.iter().map(|r| r.amount.0).sum()),
        total_sent: lws::Amount(spent_outputs.iter().map(|s| s.amount.0).sum()),
        fee: None,
        unlock_time: 0,
        height,
        spent_outputs,
        receives,
        payment_id: None,
        coinbase: false,
        mempool: height.is_none(),
        mixin: 15,
        proof: None,
    }
}

fn answer(transactions: Vec<lws::Transaction>, scanned: u64) -> lws::AddressTxs {
    lws::AddressTxs {
        total_received: lws::Amount(0),
        scanned_height: scanned,
        scanned_block_height: scanned,
        start_height: 1,
        transaction_height: scanned,
        blockchain_height: scanned + 1,
        transactions,
        lookahead: None,
    }
}

fn spend_of(paid: &Paid, image: [u8; 32]) -> lws::Spend {
    lws::Spend { amount: lws::Amount(paid.amount), key_image: lws::Hex(image), tx_pub_key: lws::Hex(paid.tx_key), out_index: paid.index, mixin: 15, sender: None }
}

/// The key image this wallet's spend key gives the output `paid` describes.
fn image_of(keys: &Keys, paid: &Paid) -> [u8; 32] {
    let (offset, _) = keys.light_output_at(&paid.tx_key, paid.index.into(), paid.account, paid.subaddress, &paid.key).unwrap();
    let key = CompressedPoint::from(paid.key).decompress().unwrap();
    keys.key_image(key, offset).unwrap()
}

/// The facts of three blocks: a reward and a payment to the main address,
/// a payment to subaddress 3, then a spend of the first payment.
struct Chain {
    blocks: Vec<(ScannableBlock, [u8; 32])>,
    /// What a server would list, mined.
    listed: Vec<lws::Transaction>,
}

fn chain(keys: &Keys) -> Chain {
    let (t1, h1, p1) = transaction(keys, Some((3 * XMR, 0)), &[]);
    let p1 = p1.unwrap();
    let (b1, m1, reward) = block(keys, 300_101, Some(600_000_000_000), vec![(t1, h1)]);
    let reward = reward.unwrap();
    let (t2, h2, p2) = transaction(keys, Some((XMR / 2, 3)), &[]);
    let p2 = p2.unwrap();
    let (b2, _, _) = block(keys, 300_102, None, vec![(t2, h2)]);
    let image = image_of(keys, &p1);
    let (t3, h3, _) = transaction(keys, None, &[image]);
    let (b3, _, _) = block(keys, 300_103, None, vec![(t3, h3)]);
    let mut coinbase = listed(m1, Some(300_101), vec![received(&reward, 3_001_010)], vec![]);
    coinbase.coinbase = true;
    coinbase.unlock_time = 300_101 + 60;
    // The spend lists the real spend of p1, and a decoy use of p2 with
    // another key image: only the first is a spend.
    let mut spend = listed(h3, Some(300_103), vec![], vec![spend_of(&p1, image), spend_of(&p2, [9; 32])]);
    spend.fee = Some(lws::Amount(1000));
    Chain {
        blocks: [b1, b2, b3].into_iter().map(|b| { let hash = b.block.hash(); (b, hash) }).collect(),
        listed: vec![coinbase, listed(h1, Some(300_101), vec![received(&p1, 3_001_011)], vec![]), listed(h2, Some(300_102), vec![received(&p2, 3_001_021)], vec![]), spend],
    }
}

/// A spend as both paths must find it: transaction, height, time, fee.
type FoundSpend = (String, u64, u64, u64);

/// What a state holds that both paths must agree on.
fn found(state: &State) -> (serde_json::Value, Vec<FoundSpend>, u64, u64, u32) {
    let outputs = serde_json::to_value(&state.outputs).unwrap();
    let spends = state.spends.iter().map(|s| (s.tx.clone(), s.height, s.time, s.fee)).collect();
    let total = state.outputs.iter().filter(|o| o.coin.is_free()).map(|o| o.amount).sum();
    (outputs, spends, total, state.spendable(), state.receive_index)
}

/// The strongest check: blocks scanned the node way, and the same facts
/// served by a fake light wallet server, come to the same state.
#[tokio::test]
async fn the_light_path_finds_what_the_node_path_finds() {
    let wallet = Wallet::create(&dir("light-equals-node"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);

    // The node way.
    let (scanner, prefilter) = wallet.scanner(&keyed::Watch::first(Wallet::watched(0)));
    let hashes: Vec<[u8; 32]> = chain.blocks.iter().map(|b| b.1).collect();
    let (results, _) = scan::scan(&scanner, &prefilter, chain.blocks.iter().map(|b| b.0.clone()).collect(), &hashes, false);
    let node_state = {
        let mut state = wallet.inner.state.lock().await;
        let mut images = Wallet::key_images(&state);
        for result in results {
            wallet.take_block(&mut state, &mut images, &HashMap::new(), result.unwrap());
        }
        state.tip = 300_300;
        assert_eq!(state.outputs.len(), 3, "the reward and two payments");
        assert_eq!(state.spends.len(), 1);
        let kept = found(&state);
        state.outputs.clear();
        state.spends.clear();
        state.receive_index = 0;
        state.restore_height = Some(300_100);
        state.next_block = 300_104;
        kept
    };

    // The light way, through a fake server listing the same facts.
    let txs = serde_json::to_string(&answer(chain.listed.clone(), 300_299)).unwrap();
    let url = scripted_node(move |text| {
        let txs = txs.clone();
        async move {
            if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":300100}"#.to_owned()
            } else if text.starts_with("POST /upsert_subaddrs") {
                r#"{"new_subaddrs":[{"key":0,"value":[[1,200]]}]}"#.to_owned()
            } else if text.starts_with("POST /get_address_txs") {
                txs
            } else {
                "{}".to_owned()
            }
        }
    })
    .await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.light_url = Some(url);
        state.sync_mode = SyncMode::Light;
    }
    assert!(!wallet.sync_step().await.unwrap(), "one poll and done");
    let state = wallet.inner.state.lock().await;
    let light_state = found(&state);
    assert_eq!(light_state.0, node_state.0, "the same outputs, byte for byte");
    assert_eq!(light_state.1, node_state.1, "the same spends, fee too");
    assert_eq!((light_state.2, light_state.3, light_state.4), (node_state.2, node_state.3, node_state.4), "the same balance and addresses");
    assert_eq!(state.spends[0].fee, 1000);
    assert!(state.light_registered);
    assert_eq!(state.light_subaddrs_sent, Wallet::watched(0), "told of the addresses watched before the poll");
    assert_eq!((state.light_scanned_height, state.tip), (300_299, 300_300));
    drop(state);
    let status = wallet.status();
    assert!(status.synced && status.connected);
    assert_eq!(fob_words::catch_up::chip(status.chip()).text, "Synced · server");
    assert_eq!(status.light.as_ref().unwrap().amounts, catch_up::Amounts::Checked, "every amount opened the chain's commitment");
}

/// The bytes Fob writes for a `WalletOutput` are those monero-wallet 0.2's
/// scanner makes, for the main address, a subaddress and a mined reward.
#[test]
fn wallet_output_bytes_are_monero_wallet_s() {
    let keys = Keys::generate(Network::Stagenet).keys;
    let chain = chain(&keys);
    let mut scanner = Scanner::new(keys.view_pair());
    for i in 1..=10 {
        scanner.register_subaddress(SubaddressIndex::new(0, i).unwrap());
    }
    let mut checked = 0;
    for (block, _) in &chain.blocks {
        for output in scanner.scan(block.clone()).unwrap().ignore_additional_timelock() {
            let timelock = output.additional_timelock();
            let parts = WalletOutputParts {
                tx: output.transaction(),
                index: output.index_in_transaction(),
                global_index: output.index_on_blockchain(),
                key: output.key().compress().to_bytes(),
                key_offset: output.key_offset(),
                mask: output.commitment().mask,
                amount: output.commitment().amount,
                timelock,
                subaddress: output.subaddress(),
                payment_id: output.payment_id(),
            };
            let bytes = wallet_output_bytes(&parts);
            assert_eq!(bytes, output.serialize());
            assert_eq!(WalletOutput::read(&mut bytes.as_slice()).unwrap().serialize(), bytes);
            checked += 1;
        }
    }
    assert_eq!(checked, 3);
}

#[test]
fn the_api_s_times_read_back() {
    assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(unix_time("2023-06-20T22:53:20Z"), Some(1_687_301_600));
    assert_eq!(unix_time(&lws::Timestamp::from_unix(1_700_012_345).0), Some(1_700_012_345));
    assert_eq!(unix_time("yesterday"), None);
}

/// A state fresh from a wallet, for `reconcile` alone.
fn fresh() -> (Wallet, State) {
    let wallet = Wallet::create(&dir(&format!("light-fresh-{}", OsRng.next_u32())), [1; 32], Network::Stagenet).unwrap();
    let state = State::clone(&wallet.inner.state.try_lock().unwrap());
    (wallet, state)
}

#[test]
fn reconcile_receives_spends_and_is_idempotent() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let response = answer(chain.listed.clone(), 400_000);
    let first = reconcile(keys, &mut state, &response, &HashMap::new(), 5).unwrap();
    assert!(first.changed);
    assert_eq!(first.refused, 0);
    assert_eq!(state.outputs.len(), 3);
    let spent: Vec<_> = state.outputs.iter().filter(|o| !o.coin.is_free()).collect();
    assert_eq!(spent.len(), 1, "the real spend only, not the decoy use");
    assert_eq!(spent[0].amount, 3 * XMR);
    assert_eq!(state.receive_index, 3);
    let coinbase = state.outputs.iter().find(|o| o.coinbase).unwrap();
    assert_eq!(coinbase.unlock_block, 300_161);
    let saved = serde_json::to_value(&state).unwrap();
    let again = reconcile(keys, &mut state, &response, &HashMap::new(), 6).unwrap();
    assert!(!again.changed, "the same answer twice changes nothing");
    assert_eq!(serde_json::to_value(&state).unwrap(), saved);
}

#[test]
fn a_transaction_the_server_drops_was_reorganised_out() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    reconcile(keys, &mut state, &answer(chain.listed.clone(), 400_000), &HashMap::new(), 5).unwrap();
    state.sent.push(Sent { height: Some(300_103), ..sent_payment(&hex::encode(chain.listed[3].hash.0)) });
    // The spend in block 103 is gone from the chain: its coin is free again,
    // the payment unmined; what's below stays.
    let shorter: Vec<_> = chain.listed[..3].to_vec();
    let done = reconcile(keys, &mut state, &answer(shorter.clone(), 400_000), &HashMap::new(), 6).unwrap();
    assert!(done.changed);
    assert!(state.spends.is_empty());
    assert_eq!(state.sent[0].height, None);
    // Its coin stays spent: this wallet's own payment, now waiting, holds it.
    assert_eq!(state.outputs.iter().filter(|o| !o.coin.is_free()).count(), 1);
    // The payment to subaddress 3 goes too: its output is dropped.
    let done = reconcile(keys, &mut state, &answer(shorter[..2].to_vec(), 400_000), &HashMap::new(), 7).unwrap();
    assert!(done.changed);
    assert_eq!(state.outputs.len(), 2);
    // A server that hasn't scanned that far (after a request to scan from
    // the start) vouches for nothing past its height: nothing more goes.
    let done = reconcile(keys, &mut state, &answer(vec![], 50), &HashMap::new(), 8).unwrap();
    assert_eq!(state.outputs.len(), 2);
    assert!(done.changed, "its height moved");
}

fn sent_payment(tx: &str) -> Sent {
    Sent {
        tx: tx.into(),
        to: "addr".into(),
        amount: XMR,
        fee: 30,
        change: 0,
        time: 1,
        height: None,
        receipt: None, receipt_view: None,
        note: None,
        pieces: None,
        not_confirmed: false,
        released: false,
        back: vec![],
        recipient_name: None,
        signed: None,
    }
}

#[test]
fn a_pool_payment_is_on_its_way_until_mined() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let (_, hash, paid) = transaction(keys, Some((XMR, 2)), &[]);
    let paid = paid.unwrap();
    let pooled = listed(hash, None, vec![received(&paid, 0)], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![pooled], 200), &HashMap::new(), 42).unwrap();
    assert!(state.outputs.is_empty(), "not owned until mined");
    assert_eq!(done.pool.get(&hex::encode(hash)), Some(&Pooled { seen: 42, parts: vec![(2, XMR)], accounts: vec![] }));
    // Seen again later: still when it was first seen.
    let pooled = listed(hash, None, vec![received(&paid, 0)], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![pooled], 200), &done.pool, 99).unwrap();
    assert_eq!(done.pool[&hex::encode(hash)].seen, 42);
    // Mined: owned, and out of the pool.
    let mined = listed(hash, Some(150), vec![received(&paid, 1500)], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![mined], 200), &done.pool, 120).unwrap();
    assert!(done.pool.is_empty());
    assert_eq!(state.outputs.len(), 1);
    assert_eq!(state.outputs[0].height, 150);
}

#[test]
fn an_output_that_isn_t_the_wallet_s_is_refused() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let (_, hash, paid) = transaction(keys, Some((XMR, 0)), &[]);
    let paid = paid.unwrap();
    // Another key: not made from the transaction key for this wallet.
    let mut forged = received(&paid, 7);
    forged.index = 1;
    forged.public_key = lws::Hex(stray_point().to_bytes());
    // The right key with another mask: a commitment that isn't on the chain.
    let mut wrong_mask = received(&paid, 8);
    wrong_mask.rct = Some(lws::Hex(<[u8; 32]>::from(Scalar::hash(b"another"))));
    let done = reconcile(keys, &mut state, &answer(vec![listed(hash, Some(300_000), vec![forged.clone(), wrong_mask.clone()], vec![])], 400_000), &HashMap::new(), 1).unwrap();
    assert_eq!(done.refused, 2);
    assert!(state.outputs.is_empty());
    // Before hard fork 10 a mask came encrypted, not from the view key: it is
    // the server's, held to the commitment when the server gives one...
    let old = |receives| answer(vec![listed(hash, Some(150), receives, vec![])], 400_000);
    assert!(matches!(reconcile(keys, &mut state, &old(vec![forged.clone(), wrong_mask.clone()]), &HashMap::new(), 1), Err(Problem::AmountMismatch)));
    assert!(state.outputs.is_empty());
    // ...and taken on its word when it gives none (a wrong one only makes the
    // coin unspendable).
    wrong_mask.commitment = None;
    let done = reconcile(keys, &mut state, &old(vec![forged, wrong_mask]), &HashMap::new(), 1).unwrap();
    assert_eq!((done.refused, state.outputs.len(), done.unchecked), (1, 1, 1));
}

/// The server holds the view key, so it can make a genuine output at any
/// subaddress: one past what the wallet watches is refused, and Receive
/// doesn't move on to it.
#[test]
fn an_output_past_the_addresses_watched_is_refused() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let before = state.receive_index;
    for sub in [u32::MAX, Wallet::watched(before) + 1] {
        let (_, hash, paid) = transaction(keys, Some((XMR, sub)), &[]);
        let done = reconcile(keys, &mut state, &answer(vec![listed(hash, Some(300_000), vec![received(&paid.unwrap(), 9)], vec![])], 400_000), &HashMap::new(), 1).unwrap();
        assert_eq!(done.refused, 1);
        assert!(state.outputs.is_empty());
        assert_eq!(state.receive_index, before);
    }
    // At the edge of what's watched: taken.
    let edge = Wallet::watched(before);
    let (_, hash, paid) = transaction(keys, Some((XMR, edge)), &[]);
    let done = reconcile(keys, &mut state, &answer(vec![listed(hash, Some(300_000), vec![received(&paid.unwrap(), 9)], vec![])], 400_000), &HashMap::new(), 1).unwrap();
    assert_eq!((done.refused, state.outputs.len(), state.receive_index), (0, 1, edge));
}

/// An amount that doesn't open the commitment the server gives: the whole
/// answer is refused, nothing in the wallet changes, the sync stops with
/// words, and the Light server screen and the chip say so.
#[tokio::test]
async fn an_amount_that_doesn_t_match_the_chain_stops_the_sync() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let mut lying = chain.listed.clone();
    // The payment to subaddress 3, a thousand times larger.
    lying[2].receives[0].amount = lws::Amount(500 * XMR);
    let before = serde_json::to_value(&state).unwrap();
    assert!(matches!(reconcile(keys, &mut state, &answer(lying.clone(), 400_000), &HashMap::new(), 5), Err(Problem::AmountMismatch)));
    assert_eq!(serde_json::to_value(&state).unwrap(), before, "nothing of the answer taken, the honest outputs neither");

    // Through a server: the poll fails with the words, and says so.
    let txs = serde_json::to_string(&answer(lying, 300_299)).unwrap();
    let url = scripted_node(move |text| {
        let txs = txs.clone();
        async move {
            if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":300100}"#.to_owned()
            } else if text.starts_with("POST /upsert_subaddrs") {
                r#"{"new_subaddrs":[]}"#.to_owned()
            } else if text.starts_with("POST /get_address_txs") {
                txs
            } else {
                "{}".to_owned()
            }
        }
    })
    .await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.light_url = Some(url);
        state.sync_mode = SyncMode::Light;
    }
    let said = wallet.sync_step().await.unwrap_err().to_string();
    assert_eq!(said, "The light server reported an amount that doesn't match the chain. Nothing from its answer was taken");
    assert!(wallet.inner.state.lock().await.outputs.is_empty());
    let light = wallet.status().light.unwrap();
    assert!(light.refused);
    assert_eq!(light.amounts, catch_up::Amounts::Mismatch);
    wallet.set_status(|s| s.error = Some(said.clone()));
    assert_eq!(fob_words::catch_up::chip(wallet.status().chip()).text, "Server answer refused");
    let standing = wallet.light_standing().await;
    assert!(standing.rows.iter().any(|r| r.label == "Amounts" && r.value == "don't match the chain" && r.bad));
    assert!(standing.rows.iter().any(|r| r.label == "Last answer" && r.value == said));
}

/// A server that serves no commitments (monero-lws): its outputs are taken,
/// and the Light server screen says their amounts are its word.
#[tokio::test]
async fn amounts_without_commitments_are_the_server_s_word() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let mut bare = chain.listed.clone();
    for tx in &mut bare {
        for r in &mut tx.receives {
            r.commitment = None;
        }
    }
    let done = reconcile(keys, &mut state, &answer(bare.clone(), 400_000), &HashMap::new(), 5).unwrap();
    assert_eq!((done.checked, done.unchecked, state.outputs.len()), (0, 3, 3));
    let done = reconcile(keys, &mut fresh().1, &answer(chain.listed.clone(), 400_000), &HashMap::new(), 5).unwrap();
    assert_eq!((done.checked, done.unchecked), (3, 0));

    let txs = serde_json::to_string(&answer(bare, 300_299)).unwrap();
    let url = scripted_node(move |text| {
        let txs = txs.clone();
        async move {
            if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":300100}"#.to_owned()
            } else if text.starts_with("POST /get_address_txs") {
                txs
            } else {
                r#"{"new_subaddrs":[]}"#.to_owned()
            }
        }
    })
    .await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.light_url = Some(url);
        state.sync_mode = SyncMode::Light;
    }
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.status().light.unwrap().amounts, catch_up::Amounts::ServersWord);
    let standing = wallet.light_standing().await;
    assert!(standing.rows.iter().any(|r| r.label == "Amounts" && r.value == "the server's word" && !r.bad));
    assert!(standing.notes.iter().any(|n| n.as_str() == fob_words::light::SERVERS_WORD));
}

/// An answer that contradicts itself is refused whole: one output named
/// twice with different keys, one place on the chain for two outputs, a
/// block well past the tip.
#[test]
fn an_answer_that_contradicts_itself_is_refused() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let before = serde_json::to_value(&state).unwrap();
    let refused = |state: &mut State, txs: Vec<lws::Transaction>| matches!(reconcile(keys, state, &answer(txs, 400_000), &HashMap::new(), 5), Err(Problem::Contradiction));

    let mut twice = chain.listed.clone();
    let mut other = twice[1].receives[0].clone();
    other.public_key = lws::Hex(stray_point().to_bytes());
    twice[1].receives.push(other);
    assert!(refused(&mut state, twice), "the same output with two keys");

    let mut one_place = chain.listed.clone();
    one_place[2].receives[0].global_index = one_place[1].receives[0].global_index;
    assert!(refused(&mut state, one_place), "two outputs at one global index");

    let mut future = chain.listed.clone();
    future[3].height = Some(400_001 + 11);
    assert!(refused(&mut state, future), "a block past the tip");
    assert_eq!(serde_json::to_value(&state).unwrap(), before);

    // A block just past the tip the server gives is its scanner ahead of
    // the height it reports: taken.
    let mut ahead = chain.listed.clone();
    ahead[3].height = Some(400_001 + 5);
    assert!(reconcile(keys, &mut state, &answer(ahead, 400_000), &HashMap::new(), 5).is_ok());
}

/// The fee the server gives is the spend's; none is 0, which Activity says
/// isn't known rather than claiming none; one given later fills it in.
#[tokio::test]
async fn a_spend_s_fee_is_the_server_s_or_not_known() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let mut without = chain.listed.clone();
    without[3].fee = None;
    reconcile(keys, &mut state, &answer(without, 400_000), &HashMap::new(), 5).unwrap();
    assert_eq!(state.spends[0].fee, 0);
    wallet.inner.state.lock().await.set(state.clone());
    let rows = wallet.activity().await;
    let out = rows.iter().find(|r| r.direction == Direction::Out).unwrap();
    assert_eq!(out.fee, None, "no fee claimed");
    let detail = fob_words::activity::detail(fob_screens::activity::detail(out, None, false, "Kraken", clock_now()));
    assert_eq!(detail.fee.as_deref(), Some("Not known · counted in the amount"));
    // Given now: filled in, once.
    let done = reconcile(keys, &mut state, &answer(chain.listed.clone(), 400_000), &HashMap::new(), 6).unwrap();
    assert!(done.changed);
    assert_eq!(state.spends[0].fee, 1000);
    wallet.inner.state.lock().await.set(state);
    let rows = wallet.activity().await;
    let out = rows.iter().find(|r| r.direction == Direction::Out).unwrap();
    assert_eq!(out.fee, Some(1000));
    let detail = fob_words::activity::detail(fob_screens::activity::detail(out, None, false, "Kraken", clock_now()));
    assert_eq!(detail.fee.as_deref(), Some("0.000000001 XMR"));
}

/// An output locked to a time, and one locked to a block, found the node
/// way and through a server, come out the same byte for byte; the time lock
/// is judged by the chain's clock, which the server's block times move on,
/// but not one past the phone's.
#[tokio::test]
async fn a_locked_output_is_kept_alike_both_ways() {
    let wallet = Wallet::create(&dir("light-locked"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let until = 1_900_000_000;
    let (t1, h1, p1) = locked_transaction(keys, Some((2 * XMR, 0)), &[], Timelock::Time(until));
    let (t2, h2, p2) = locked_transaction(keys, Some((XMR, 4)), &[], Timelock::Block(300_500));
    let (b1, _, _) = block(keys, 300_201, None, vec![(t1, h1), (t2, h2)]);
    let hash = b1.block.hash();

    let (scanner, prefilter) = wallet.scanner(&keyed::Watch::first(Wallet::watched(0)));
    let (results, _) = scan::scan(&scanner, &prefilter, vec![b1], &[hash], false);
    let node = {
        let mut state = wallet.inner.state.lock().await;
        let mut images = Wallet::key_images(&state);
        for result in results {
            wallet.take_block(&mut state, &mut images, &HashMap::new(), result.unwrap());
        }
        assert_eq!(state.outputs.len(), 2, "both kept the node way");
        let kept = serde_json::to_value(&state.outputs).unwrap();
        state.outputs.clear();
        state.receive_index = 0;
        kept
    };

    let mut locked = listed(h1, Some(300_201), vec![received(&p1.unwrap(), 3_002_011)], vec![]);
    locked.unlock_time = until;
    let mut later = listed(h2, Some(300_201), vec![received(&p2.unwrap(), 3_002_012)], vec![]);
    later.unlock_time = 300_500;
    let mut state = wallet.inner.state.lock().await;
    let now = 1_700_000_000 + 300_201 * 120 + 30;
    let done = reconcile(keys, &mut state, &answer(vec![locked, later], 300_300), &HashMap::new(), now).unwrap();
    assert_eq!(done.refused, 0);
    assert_eq!(serde_json::to_value(&state.outputs).unwrap(), node, "the same outputs, byte for byte");
    let time_locked = state.outputs.iter().find(|o| o.unlock_time().is_some()).unwrap().clone();
    assert_eq!(time_locked.unlock_block, until);
    assert_eq!(state.last_block_time, 1_700_000_000 + 300_201 * 120, "the latest block time vouched for");
    assert!(state.needed(&time_locked) > confirmations(&state, time_locked.height), "not ready before its time");
    // A server's block time past the phone's clock doesn't move it.
    let mut ahead = listed([5; 32], Some(300_250), vec![], vec![]);
    ahead.timestamp = Some(lws::Timestamp::from_unix(until + 10));
    reconcile(keys, &mut state, &answer(vec![ahead], 300_300), &HashMap::new(), now + 60).unwrap();
    assert_eq!(state.last_block_time, 1_700_000_000 + 300_201 * 120);
}

/// A version 7 file, from before light mode, opens as a node wallet with
/// nothing lost; the setting then saves and reads back.
#[tokio::test]
async fn a_version_7_database_opens_as_a_node_wallet() {
    let path = dir("db-version-7");
    let wallet = a_used_wallet(&path).await;
    let state = saved(&wallet).await;
    drop(wallet);
    let old = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    old.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\"; ALTER TABLE wallet DROP COLUMN sync_mode; ALTER TABLE wallet DROP COLUMN light_url; ALTER TABLE wallet DROP COLUMN light_self_signed; ALTER TABLE wallet DROP COLUMN light_pin; ALTER TABLE wallet DROP COLUMN light_registered; ALTER TABLE wallet DROP COLUMN light_subaddrs_sent; ALTER TABLE wallet DROP COLUMN light_scanned_height; ALTER TABLE wallet DROP COLUMN light_blocks; ALTER TABLE wallet DROP COLUMN light_history; {BACK_TO_VERSION_11} {BACK_TO_VERSION_9} PRAGMA user_version = 7;",
        hex::encode([1u8; 32])
    ))
    .unwrap();
    drop(old);
    let upgraded = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(saved(&upgraded).await, state);
    assert_eq!(upgraded.backend().await, crate::light::Backend::Node);
    {
        let mut state = upgraded.inner.state.lock().await;
        state.sync_mode = SyncMode::Light;
        state.light_url = Some("https://lws.example:8443".into());
        state.light_subaddrs_sent = 250;
        state.light_scanned_height = 2_250_000;
        upgraded.save_state(&state).unwrap();
    }
    drop(upgraded);
    let reopened = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(reopened.backend().await, crate::light::Backend::Light { url: "https://lws.example:8443".into(), self_signed: false });
    let state = reopened.inner.state.lock().await;
    assert_eq!((state.light_subaddrs_sent, state.light_scanned_height), (250, 2_250_000));
}

/// A server on 127.0.0.1 answering each request with the status and body
/// `answer` gives for its route. Gives its address.
async fn status_server(answer: impl Fn(&str) -> (u16, String) + Send + Sync + 'static) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let answer = answer.clone();
            tokio::spawn(async move {
                let mut request = vec![];
                let mut buf = [0; 4096];
                loop {
                    let Ok(n) = stream.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&request).into_owned();
                let route = text.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_owned();
                let (status, body) = answer(&route);
                let response = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    url
}

/// What a stagenet server says of itself.
const VERSION: &str = r#"{"server_type":"monokulo-lws","monero_version_full":"","blockchain_height":2250000,"api":2,"max_subaddresses":10000,"network":"stage","testnet":true}"#;

/// A stagenet server with an empty history, that approves the wallet once
/// `approved` is set (until then `/login` answers 403), and counts each
/// `/upsert_subaddrs`.
async fn light_server(approved: Arc<AtomicBool>, upserts: Arc<AtomicU64>) -> String {
    status_server(move |route| match route {
        "get_version" => (200, VERSION.to_owned()),
        "login" if approved.load(Ordering::Relaxed) => (200, r#"{"new_address":true,"start_height":2249000}"#.to_owned()),
        "login" => (403, "{}".to_owned()),
        "upsert_subaddrs" => {
            upserts.fetch_add(1, Ordering::Relaxed);
            (200, r#"{"new_subaddrs":[]}"#.to_owned())
        }
        "get_address_txs" => (200, r#"{"total_received":"0","scanned_height":2249999,"scanned_block_height":2249999,"start_height":2249000,"transaction_height":2249999,"blockchain_height":2250000,"transactions":[]}"#.to_owned()),
        _ => (404, String::new()),
    })
    .await
}

/// Choosing a server and switching to it: refused without a server, on
/// another network or onion; on, ZMQ is off and Manual Sync moot, and Send
/// open (with nothing to send here); off, node scanning starts from the
/// server's height less 30.
#[tokio::test]
async fn switching_to_a_server_and_back() {
    let wallet = Wallet::create(&dir("light-switch"), [1; 32], Network::Stagenet).unwrap();
    assert!(matches!(wallet.use_light_wallet(true).await, Err(FobError::Light(crate::light::Problem::NoServerSet))));
    assert!(matches!(wallet.set_light_server("http://lws.abcdefghijklmnop.onion", false).await, Err(FobError::Light(crate::light::Problem::Onion))));
    let upserts = Arc::new(AtomicU64::new(0));
    let url = light_server(Arc::new(AtomicBool::new(true)), upserts.clone()).await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.restore_height = Some(2_240_000);
        state.next_block = 2_249_500;
        state.recent.push((2_249_499, "aa".into()));
        state.zmq_on = true;
        state.zmq_endpoint = Some("tcp://stagenet.xmr-tw.org:38083".into());
        state.manual_sync = true;
    }
    wallet.set_light_server(&url, false).await.unwrap();
    assert_eq!(wallet.light_line().await.unwrap(), node::address(&url), "chosen, not used yet");
    wallet.use_light_wallet(true).await.unwrap();
    assert!(wallet.inner.state.lock().await.recent.is_empty(), "the block anchors go");
    assert_eq!(wallet.status().zmq, ZmqStatus::Off);
    assert!(!wallet.waiting_to_be_asked().await, "Manual Sync is moot");
    wallet.sync_now().await;
    let status = wallet.status();
    assert!(status.synced, "{status:?}");
    assert_eq!(fob_words::catch_up::chip(status.chip()).text, "Synced · server");
    assert_eq!(wallet.light_line().await.unwrap(), format!("{} · synced", node::address(&url)));
    assert!(matches!(wallet.prepare(&wallet.primary_address(), 1, None, None).await, Err(FobError::Screen(ScreenError::NotEnoughFunds { .. }))));

    wallet.use_light_wallet(false).await.unwrap();
    let state = wallet.inner.state.lock().await;
    assert_eq!(state.next_block, 2_249_999 - 30, "the server's height less decision 9's margin");
    assert_eq!(state.sync_mode, SyncMode::Node);
    drop(state);
    assert!(wallet.status().light.is_none());
}

#[tokio::test]
async fn a_server_on_another_network_or_without_subaddresses_is_refused() {
    let wallet = Wallet::create(&dir("light-refused"), [1; 32], Network::Mainnet).unwrap();
    let url = light_server(Arc::new(AtomicBool::new(true)), Arc::default()).await;
    match wallet.set_light_server(&url, false).await {
        Err(FobError::Light(crate::light::Problem::OtherNetwork { theirs, ours })) => assert_eq!((theirs.as_str(), ours.as_str()), ("stage", "main")),
        other => panic!("expected another network, got {other:?}"),
    }
    assert!(wallet.light_server().await.is_none());
}

/// A server that hasn't approved the wallet: the chip waits, `/login` is
/// asked again each poll, and once approved the wallet is told of its
/// addresses and syncs.
#[tokio::test]
async fn waiting_for_the_server_to_approve() {
    let wallet = Wallet::create(&dir("light-approve"), [1; 32], Network::Stagenet).unwrap();
    let (approved, upserts) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)));
    let url = light_server(approved.clone(), upserts.clone()).await;
    wallet.set_light_server(&url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    wallet.sync_now().await;
    let status = wallet.status();
    assert!(status.error.is_none());
    assert_eq!(fob_words::catch_up::chip(status.chip()).text, "Waiting for approval");
    assert_eq!(wallet.light_line().await.unwrap(), format!("{} · waiting for approval", node::address(&url)));
    assert_eq!(upserts.load(Ordering::Relaxed), 0);
    approved.store(true, Ordering::Relaxed);
    wallet.sync_now().await;
    assert_eq!(fob_words::catch_up::chip(wallet.status().chip()).text, "Synced · server");
    assert_eq!(upserts.load(Ordering::Relaxed), 1);
    // Told once: the next poll sends nothing new.
    wallet.sync_now().await;
    assert_eq!(upserts.load(Ordering::Relaxed), 1);
    // A new wallet that never synced starts where the server does.
    assert_eq!(wallet.inner.state.lock().await.restore_height, Some(2_249_000));
}

/// A server that takes no new wallets, or has subaddresses off once asked:
/// said in words.
#[tokio::test]
async fn a_server_that_refuses_says_why() {
    let closed = status_server(|route| match route {
        "get_version" => (200, VERSION.to_owned()),
        "login" => (501, "{}".to_owned()),
        _ => (404, String::new()),
    })
    .await;
    let wallet = Wallet::create(&dir("light-closed"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_light_server(&closed, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    let said = wallet.sync_step().await.unwrap_err().to_string();
    assert_eq!(said, "This server doesn't take new wallets");

    let no_subaddresses = status_server(|route| match route {
        "get_version" => (200, VERSION.to_owned()),
        "login" => (200, r#"{"new_address":false,"start_height":1}"#.to_owned()),
        "upsert_subaddrs" => (409, r#"{"message":"subaddresses are off on this server"}"#.to_owned()),
        _ => (404, String::new()),
    })
    .await;
    wallet.set_light_server(&no_subaddresses, false).await.unwrap();
    let said = wallet.sync_step().await.unwrap_err().to_string();
    assert_eq!(said, "This server won't watch this wallet's addresses: subaddresses are off on this server");
}

/// `chain`'s transactions as a server that proves them lists them: each
/// with the block it's in and its pruned parts (the reward with its block's
/// miner transaction alone).
fn with_proofs(chain: &Chain) -> Vec<lws::Transaction> {
    let mut listed = chain.listed.clone();
    let block = |i: usize| &chain.blocks[i].0;
    listed[0].proof = Some(proof::make(&block(0).block, None));
    listed[1].proof = Some(proof::make(&block(0).block, Some((&block(0).transactions[0], [0; 32]))));
    listed[2].proof = Some(proof::make(&block(1).block, Some((&block(1).transactions[0], [0; 32]))));
    listed[3].proof = Some(proof::make(&block(2).block, Some((&block(2).transactions[0], [0; 32]))));
    listed
}

/// What a node vouches for of `chain`: each block's hash at its height.
fn node_knows(chain: &Chain) -> Known {
    Known { blocks: chain.blocks.iter().map(|(b, hash)| (b.block.number() as u64, *hash)).collect(), history: vec![] }
}

/// A server that holds the view key makes up a payment to this wallet
/// whose one-time key, mask and commitment all fit, in a block of its own
/// making, with a proof that holds as far as it goes: `(transaction, its
/// listing)`. The block isn't the chain's.
fn made_up(keys: &Keys, height: usize) -> lws::Transaction {
    let (tx, hash, paid) = transaction(keys, Some((100 * XMR, 0)), &[]);
    let (block, _, _) = block(keys, height, None, vec![(tx.clone(), hash)]);
    let mut listed = listed(hash, Some(height as u64), vec![received(&paid.unwrap(), height as u64 * 10 + 1)], vec![]);
    listed.fee = Some(lws::Amount(1000));
    listed.proof = Some(proof::make(&block.block, Some((&tx, [0; 32]))));
    listed
}

/// Proofs whose blocks a node has: every output, the reward included
/// (proved by its block's miner transaction), counted as matching the
/// chain; the same facts found as the node path finds them.
#[test]
fn proofs_whose_blocks_the_node_has_match_the_chain() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let proved = with_proofs(&chain);
    let done = reconcile_proven(keys, &mut state, &answer(proved.clone(), 400_000), &HashMap::new(), 5, &node_knows(&chain)).unwrap();
    assert_eq!((done.proven, done.pending, done.checked, done.unchecked, done.refused), (3, 0, 0, 0, 0));
    let mut plain = fresh().1;
    reconcile(keys, &mut plain, &answer(chain.listed.clone(), 400_000), &HashMap::new(), 5).unwrap();
    assert_eq!(found(&state), found(&plain), "a proof changes what's checked, not what's found");

    // The reward alone, proved by its block's miner transaction.
    let reward = vec![proved[0].clone()];
    let done = reconcile_proven(keys, &mut fresh().1, &answer(reward, 400_000), &HashMap::new(), 5, &node_knows(&chain)).unwrap();
    assert_eq!((done.proven, done.pending), (1, 0));

    // No node's word yet: taken, but pending.
    let done = reconcile_proven(keys, &mut fresh().1, &answer(proved, 400_000), &HashMap::new(), 5, &Known::default()).unwrap();
    assert_eq!((done.proven, done.pending), (0, 3));
}

/// A made-up payment whose key, mask and commitment fit, in a block the
/// node doesn't have at that height: refused whole, the honest outputs
/// beside it too.
#[test]
fn a_made_up_payment_in_a_block_the_node_hasn_t_is_refused() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let mut txs = with_proofs(&chain);
    txs.push(made_up(keys, 300_102));
    let before = serde_json::to_value(&state).unwrap();
    let refused = reconcile_proven(keys, &mut state, &answer(txs.clone(), 400_000), &HashMap::new(), 5, &node_knows(&chain));
    assert!(matches!(refused, Err(Problem::Contradiction)), "two blocks at 300,102 in one answer");
    // In a block of its own at a height of its own: the key, mask,
    // commitment and proof checks all pass, and only the chain tells.
    txs.pop();
    txs.push(made_up(keys, 300_150));
    assert_eq!(reconcile(keys, &mut state.clone(), &answer(txs.clone(), 400_000), &HashMap::new(), 5).map(|d| d.pending).ok(), Some(4));
    let mut node = node_knows(&chain);
    node.blocks.insert(300_150, [7; 32]);
    let refused = reconcile_proven(keys, &mut state, &answer(txs, 400_000), &HashMap::new(), 5, &node);
    assert!(matches!(refused, Err(Problem::NotOnChain)));
    assert_eq!(serde_json::to_value(&state).unwrap(), before, "nothing of the answer taken");
}

/// A proof with a byte of its transaction's prefix changed (another
/// one-time key), or one that names another output: refused whole.
#[test]
fn a_tampered_proof_is_refused() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let node = node_knows(&chain);
    let mut tampered = with_proofs(&chain);
    let key = tampered[2].receives[0].public_key.0;
    let proof = tampered[2].proof.as_mut().unwrap();
    let prefix = proof.tx_prefix.as_mut().unwrap();
    let at = prefix.0.windows(32).position(|w| w == key).unwrap();
    prefix.0[at] ^= 1;
    assert!(matches!(reconcile_proven(keys, &mut state, &answer(tampered, 400_000), &HashMap::new(), 5, &node), Err(Problem::NotOnChain)));
    // The proof of another transaction in the same block.
    let mut swapped = with_proofs(&chain);
    swapped[1].proof = swapped[0].proof.clone();
    assert!(matches!(reconcile_proven(keys, &mut state, &answer(swapped, 400_000), &HashMap::new(), 5, &node), Err(Problem::NotOnChain)));
    assert!(state.outputs.is_empty());
}

/// A node on 127.0.0.1 holding a stagenet chain of `chain` (block 0 the
/// stagenet's first, the rest made up but for what a test puts in): it
/// answers `/get_hashes.bin` as monerod does and `on_get_block_hash`,
/// counting requests and keeping the heights asked by themselves.
struct HashNode {
    url: String,
    chain: Arc<std::sync::Mutex<Vec<[u8; 32]>>>,
    requests: Arc<AtomicU64>,
    asked: Arc<std::sync::Mutex<Vec<u64>>>,
}

async fn hash_node(height: u64, blocks: &[(u64, [u8; 32])]) -> HashNode {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut hashes: Vec<[u8; 32]> = (0..height).map(|h| { let mut x = [0xee; 32]; x[..8].copy_from_slice(&h.to_le_bytes()); x }).collect();
    hashes[0] = hashes::genesis(Network::Stagenet);
    for (h, hash) in blocks {
        hashes[*h as usize] = *hash;
    }
    let chain = Arc::new(std::sync::Mutex::new(hashes));
    let (requests, asked) = (Arc::new(AtomicU64::new(0)), Arc::new(std::sync::Mutex::new(vec![])));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let node = HashNode { url, chain: chain.clone(), requests: requests.clone(), asked: asked.clone() };
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let (chain, requests, asked) = (chain.clone(), requests.clone(), asked.clone());
            tokio::spawn(async move {
                let mut request = vec![];
                let mut buf = [0; 4096];
                let body_at = loop {
                    let Ok(n) = stream.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break end + 4;
                    }
                };
                requests.fetch_add(1, Ordering::Relaxed);
                let head = String::from_utf8_lossy(&request[..body_at]).into_owned();
                let body = &request[body_at..];
                let answer: Vec<u8> = if head.starts_with("POST /get_hashes.bin") {
                    let chain = chain.lock().unwrap();
                    hashes::response(&hashes::answer(&chain, &hashes::known_in(body)))
                } else {
                    let asked_for: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
                    let height = asked_for["params"][0].as_u64().unwrap_or(u64::MAX);
                    asked.lock().unwrap().push(height);
                    let hash = chain.lock().unwrap().get(height as usize).map(hex::encode);
                    serde_json::json!({"jsonrpc": "2.0", "id": "0", "result": hash}).to_string().into_bytes()
                };
                let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", answer.len()).into_bytes();
                response.extend(answer);
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    node
}

/// A server listing `txs` (scanned to `scanned`), for `wallet` to sync
/// through, with `node` its node.
async fn through(wallet: &Wallet, txs: Vec<lws::Transaction>, scanned: u64, node: &str) {
    let txs = serde_json::to_string(&answer(txs, scanned)).unwrap();
    let url = scripted_node(move |text| {
        let txs = txs.clone();
        async move {
            if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":300100}"#.to_owned()
            } else if text.starts_with("POST /get_address_txs") {
                txs
            } else {
                r#"{"new_subaddrs":[]}"#.to_owned()
            }
        }
    })
    .await;
    let mut state = wallet.inner.state.lock().await;
    state.light_url = Some(url);
    state.sync_mode = SyncMode::Light;
    state.node = node.to_owned();
}

/// Through a server that proves its payments and a node that has their
/// blocks: they match the chain, the node was asked one stretch from the
/// wallet's first block (no payment's block by itself), and the next poll
/// asks it nothing.
#[tokio::test]
async fn a_poll_checks_proofs_with_the_node_once() {
    let wallet = Wallet::create(&dir("light-proofs"), [1; 32], Network::Stagenet).unwrap();
    let chain = chain(&wallet.inner.keys);
    let node = hash_node(300_300, &chain.blocks.iter().map(|(b, h)| (b.block.number() as u64, *h)).collect::<Vec<_>>()).await;
    through(&wallet, with_proofs(&chain), 300_299, &node.url).await;
    wallet.sync_step().await.unwrap();
    let light = wallet.status().light.unwrap();
    assert_eq!(light.amounts, catch_up::Amounts::Chain);
    assert!(!light.proof_node_unreached && !light.proof_node_default);
    assert_eq!(*node.asked.lock().unwrap(), vec![300_100], "only where the stretch starts: the wallet's first block");
    assert_eq!(wallet.inner.state.lock().await.outputs.len(), 3);
    let standing = wallet.light_standing().await;
    assert!(standing.rows.iter().any(|r| r.label == "Amounts" && r.value == "match the chain"));
    assert!(standing.rows.iter().any(|r| r.label == fob_words::light::NODE_LABEL && r.value == node::address(&node.url)));
    // Kept: the next poll, and one after reopening, ask the node nothing.
    let requests = node.requests.load(Ordering::Relaxed);
    wallet.sync_step().await.unwrap();
    assert_eq!(node.requests.load(Ordering::Relaxed), requests, "nothing asked again");
    assert_eq!(wallet.status().light.unwrap().amounts, catch_up::Amounts::Chain);
    let saved = saved(&wallet).await;
    assert_eq!(saved["light_blocks"].as_array().unwrap().len(), 3, "kept in the file");
    assert!(!saved["light_history"].as_array().unwrap().is_empty());

    // One transaction without its proof: commitments only, for them all.
    let mut one_bare = with_proofs(&chain);
    one_bare[2].proof = None;
    through(&wallet, one_bare, 300_299, &node.url).await;
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.status().light.unwrap().amounts, catch_up::Amounts::Checked);
    assert!(wallet.light_standing().await.rows.iter().any(|r| r.label == "Amounts" && r.value == "commitments only"));
}

/// A made-up payment through a server, in a block the node hasn't got:
/// the poll fails with words, nothing is taken, and the chip and the Light
/// server screen say so.
#[tokio::test]
async fn a_lying_server_s_answer_is_refused_with_words() {
    let wallet = Wallet::create(&dir("light-lying"), [1; 32], Network::Stagenet).unwrap();
    let chain = chain(&wallet.inner.keys);
    let node = hash_node(300_300, &chain.blocks.iter().map(|(b, h)| (b.block.number() as u64, *h)).collect::<Vec<_>>()).await;
    let mut txs = with_proofs(&chain);
    txs.push(made_up(&wallet.inner.keys, 300_150));
    through(&wallet, txs, 300_299, &node.url).await;
    let said = wallet.sync_step().await.unwrap_err().to_string();
    assert_eq!(said, fob_words::light::NOT_ON_CHAIN);
    assert!(wallet.inner.state.lock().await.outputs.is_empty(), "nothing taken, the honest outputs neither");
    let light = wallet.status().light.unwrap();
    assert!(light.refused);
    assert_eq!(light.amounts, catch_up::Amounts::Mismatch);
    wallet.set_status(|s| s.error = Some(said.clone()));
    assert_eq!(fob_words::catch_up::chip(wallet.status().chip()).text, "Server answer refused");
    let standing = wallet.light_standing().await;
    assert!(standing.rows.iter().any(|r| r.label == "Amounts" && r.value == "don't match the chain" && r.bad));
    assert!(*node.asked.lock().unwrap() == vec![300_100], "the made-up block's height wasn't asked by itself");
}

/// A server making up proofs across the whole chain: for payments that
/// aren't this wallet's, the node is asked nothing more; for ones that
/// fit its keys, a few windows a poll, and once the node has said
/// otherwise, nothing more at the next poll.
#[tokio::test]
async fn made_up_proofs_across_the_chain_cost_the_node_little() {
    let wallet = Wallet::create(&dir("light-many-lies"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let other = Wallet::create(&dir("light-many-lies-other"), [2; 32], Network::Stagenet).unwrap();
    let chain = chain(keys);
    let node = hash_node(300_300, &chain.blocks.iter().map(|(b, h)| (b.block.number() as u64, *h)).collect::<Vec<_>>()).await;
    let mut txs = with_proofs(&chain);
    txs.extend((1..60).map(|i| made_up(&other.inner.keys, i * 5_000)));
    through(&wallet, txs, 300_299, &node.url).await;
    assert!(matches!(wallet.sync_step().await, Err(FobError::Light(Problem::BadOutputs(59)))));
    assert_eq!(*node.asked.lock().unwrap(), vec![300_100], "only the wallet's own payments' stretch");
    assert_eq!(wallet.inner.state.lock().await.outputs.len(), 3);

    let mut txs = with_proofs(&chain);
    txs.extend((1..60).map(|i| made_up(keys, i * 5_000)));
    through(&wallet, txs.clone(), 300_299, &node.url).await;
    let before = node.requests.load(Ordering::Relaxed);
    assert!(wallet.sync_step().await.is_err());
    let first = node.requests.load(Ordering::Relaxed) - before;
    assert!(first <= 2 * hashes::WINDOWS as u64 + 2, "{first} requests");
    through(&wallet, txs, 300_299, &node.url).await;
    let before = node.requests.load(Ordering::Relaxed);
    assert!(wallet.sync_step().await.is_err());
    assert!(node.requests.load(Ordering::Relaxed) - before <= 1, "only whether the chain was reorganised");
    assert_eq!(wallet.inner.state.lock().await.outputs.len(), 3, "nothing made up taken");
}

/// What a poll opened isn't opened again at the next: only the outputs its
/// answer lists are kept, and an output that changes in any way is opened
/// afresh.
#[test]
fn a_poll_opens_only_what_the_last_didn_t() {
    use crate::wallet::light::{OpenCache, Opened, check};
    let (wallet, _) = fresh();
    let keys = &wallet.inner.keys;
    let answer = answer(with_proofs(&chain(keys)), 400_000);
    let ours = |c: &crate::wallet::light::Checked| c.opened.iter().flatten().filter(|o| matches!(o, Opened::Ours { .. })).count();
    let (first, kept) = check(keys, &answer, OpenCache::default());
    assert_eq!((ours(&first), first.images.len()), (3, 3));
    // Opened with another wallet's keys from what was kept: nothing is
    // opened again, so they read as before...
    let other = Wallet::create(&dir("light-cache-other"), [2; 32], Network::Stagenet).unwrap();
    let (again, kept) = check(&other.inner.keys, &answer, kept);
    assert_eq!(ours(&again), 3);
    // ...and an output listed otherwise is opened afresh.
    let mut changed = answer.clone();
    changed.transactions[2].receives[0].amount.0 += 1;
    changed.transactions[2].proof = None;
    changed.transactions[2].receives[0].commitment = None;
    let (afresh, _) = check(&other.inner.keys, &changed, kept);
    assert_eq!(ours(&afresh), 2);
}

/// A node that can't be reached isn't the server's fault: the payments are
/// taken, not yet checked, and the screen says the node couldn't be
/// reached; the next poll with the node back checks them.
#[tokio::test]
async fn an_unreachable_node_leaves_proofs_pending_and_refuses_nothing() {
    let wallet = Wallet::create(&dir("light-no-node"), [1; 32], Network::Stagenet).unwrap();
    let chain = chain(&wallet.inner.keys);
    // Nothing listens on port 1.
    through(&wallet, with_proofs(&chain), 300_299, "http://127.0.0.1:1").await;
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.inner.state.lock().await.outputs.len(), 3, "taken");
    let light = wallet.status().light.unwrap();
    assert!(!light.refused && light.proof_node_unreached);
    assert_eq!(light.amounts, catch_up::Amounts::ChainPending);
    assert!(wallet.status().error.is_none());
    let standing = wallet.light_standing().await;
    assert!(standing.rows.iter().any(|r| r.label == "Amounts" && r.value == "not checked yet: can't reach the node"));
    assert!(standing.notes.iter().any(|n| n.as_str() == fob_words::light::NODE_UNREACHED));

    let node = hash_node(300_300, &chain.blocks.iter().map(|(b, h)| (b.block.number() as u64, *h)).collect::<Vec<_>>()).await;
    wallet.inner.state.lock().await.node = node.url.clone();
    wallet.sync_step().await.unwrap();
    let light = wallet.status().light.unwrap();
    assert_eq!(light.amounts, catch_up::Amounts::Chain);
    assert!(!light.proof_node_unreached);
}

/// A wallet with no node of its own checks proofs with Fob's default for
/// its network, and says so.
#[tokio::test]
async fn without_a_node_the_default_checks_proofs() {
    let wallet = Wallet::create(&dir("light-default-node"), [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.node = String::new();
    let (setting, pin, default) = wallet.proof_node().await;
    assert_eq!((setting.url.as_str(), pin, default), (node::normalize(Network::Stagenet.default_nodes()[0]).as_str(), None, true));
    wallet.inner.state.lock().await.node = "http://node.example:38081".into();
    assert!(!wallet.proof_node().await.2);
}

/// A reorganisation after a payment was checked: the server lists it in
/// the new block at that height, and the node, asked on from where Fob
/// stopped, has the new chain: checked again, and it matches.
#[tokio::test]
async fn a_reorganised_block_is_checked_again() {
    let wallet = Wallet::create(&dir("light-reorg"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let blocks: Vec<(u64, [u8; 32])> = chain.blocks.iter().map(|(b, h)| (b.block.number() as u64, *h)).collect();
    let node = hash_node(300_110, &blocks).await;
    through(&wallet, with_proofs(&chain), 300_109, &node.url).await;
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.status().light.unwrap().amounts, catch_up::Amounts::Chain);
    // Block 300,102 and those after it replaced, the chain grown; the
    // payment to subaddress 3 is now in another block at 300,102.
    let p2 = chain.listed[2].receives[0].clone();
    let (t2, h2) = (chain.blocks[1].0.transactions[0].clone(), chain.listed[2].hash.0);
    let (b2, _, _) = block(keys, 300_102, None, vec![(t2.clone(), h2)]);
    let mut b2 = b2;
    b2.block.header.nonce = 77;
    {
        let mut hashes = node.chain.lock().unwrap();
        for h in 300_102..300_110 {
            hashes[h] = [0xdd; 32];
        }
        hashes[300_102] = b2.block.hash();
        hashes.extend((300_110..300_120).map(|_| [0xcc; 32]));
    }
    let mut txs = with_proofs(&chain);
    txs.truncate(2);
    let mut moved = listed(h2, Some(300_102), vec![p2], vec![]);
    moved.fee = Some(lws::Amount(1000));
    moved.proof = Some(proof::make(&b2.block, Some((&t2, [0; 32]))));
    txs.push(moved);
    let asked_before = node.asked.lock().unwrap().len();
    through(&wallet, txs, 300_119, &node.url).await;
    wallet.sync_step().await.unwrap();
    assert_eq!(wallet.status().light.unwrap().amounts, catch_up::Amounts::Chain, "checked again on the new chain");
    assert_eq!(node.asked.lock().unwrap().len(), asked_before, "on from the blocks kept near the tip: no height asked by itself");
    let saved = saved(&wallet).await;
    assert!(saved["light_blocks"].as_array().unwrap().contains(&serde_json::json!([300_102, hex::encode(b2.block.hash())])));
}

/// What a live run prints of the wallet at each step.
async fn live_report(wallet: &Wallet, step: &str) {
    let status = wallet.status();
    let chip = fob_words::catch_up::chip(status.chip());
    let state = wallet.inner.state.lock().await;
    println!(
        "[{step}] chip {:?} · scanned {} tip {} synced {} connected {} error {:?} · registered {} subaddrs_sent {} light_scanned {} next_block {} restore {:?} · outputs {} pool {:?} · amounts {:?} refused {}",
        chip.text,
        status.scanned,
        status.tip,
        status.synced,
        status.connected,
        status.error,
        state.light_registered,
        state.light_subaddrs_sent,
        state.light_scanned_height,
        state.next_block,
        state.restore_height,
        state.outputs.len(),
        wallet.inner.pool.lock().unwrap().clone(),
        status.light.as_ref().map(|l| l.amounts),
        status.light.as_ref().is_some_and(|l| l.refused),
    );
    for o in &state.outputs {
        println!("    output {}… at {} · {} XMR · subaddress {} · unlock {} · {}", &o.tx[..12], o.height, fob_monero::amount::format(o.amount), o.subaddress, o.unlock_block, o.coin);
    }
    for sp in &state.spends {
        println!("    spend {}… at {} · fee {}", &sp.tx[..12], sp.height, if sp.fee == 0 { "not known".to_owned() } else { format!("{} XMR", fob_monero::amount::format(sp.fee)) });
    }
}

/// A fresh stagenet wallet through a monokulo-lws running at `LWS_URL`
/// (`monokulo-lws serve --create auto …`): registered, told of its
/// subaddresses, synced at the tip with nothing in it; then back to the
/// node, which scans from the server's height less 30.
#[tokio::test]
#[ignore = "needs monokulo-lws on stagenet at LWS_URL, and the network"]
async fn live_a_fresh_wallet_through_monokulo_lws() {
    let url = std::env::var("LWS_URL").unwrap_or_else(|_| "http://127.0.0.1:18443".into());
    let wallet = Wallet::create(&dir("light-live-fresh"), [1; 32], Network::Stagenet).unwrap();
    println!("address {}", wallet.primary_address());
    let report = wallet.check_light_server(&url, false).await;
    println!("test: {:?}", fob_words::light::report(&report));
    wallet.set_light_server(&url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    live_report(&wallet, "light on").await;
    for poll in 1..=12 {
        let more = wallet.sync_step().await;
        live_report(&wallet, &format!("poll {poll}: {more:?}")).await;
        if wallet.status().synced {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let status = wallet.status();
    assert!(status.synced, "synced at the tip through the server");
    assert!(wallet.inner.state.lock().await.light_registered);
    assert_eq!(wallet.inner.state.lock().await.light_subaddrs_sent, Wallet::watched(0));
    assert!(wallet.lists(clock_now()).await.days.is_empty(), "an empty activity");
    println!("chip: {:?}", fob_words::catch_up::chip(status.chip()));
    println!("details: {:?}", fob_words::catch_up::details(wallet.sync_details(clock_now())));
    println!("settings row: {:?}", wallet.light_line().await);

    let scanned = wallet.inner.state.lock().await.light_scanned_height;
    wallet.use_light_wallet(false).await.unwrap();
    live_report(&wallet, "light off").await;
    let restore = wallet.inner.state.lock().await.restore_height.unwrap_or(0);
    assert_eq!(wallet.inner.state.lock().await.next_block, (scanned - 30).max(restore), "the server's height less 30, never before the wallet began");
    // The node takes over from there.
    let mut steps = 0;
    loop {
        let more = wallet.sync_step().await.unwrap();
        steps += 1;
        live_report(&wallet, &format!("node step {steps}")).await;
        if !more || steps > 20 {
            break;
        }
    }
    let status = wallet.status();
    assert!(status.synced, "the node caught up from the server's height");
    assert!(wallet.status().light.is_none());
}

/// The wallet in `LIVE_SPEND`/`LIVE_VIEW` (a stagenet test wallet, already
/// known to the server at `LWS_URL`), synced through the server alone from
/// `LIVE_FROM`, its proofs checked with the node at `LIVE_NODE` (Fob's
/// default otherwise): every output it lists comes with a proof whose
/// block the node has, so they match the chain, every spend with a fee, and
/// nothing is refused.
#[tokio::test]
#[ignore = "needs monokulo-lws on stagenet at LWS_URL, a test wallet's keys, and the network"]
async fn live_an_existing_wallet_through_monokulo_lws() {
    let url = std::env::var("LWS_URL").unwrap_or_else(|_| "http://127.0.0.1:18443".into());
    let spend = std::env::var("LIVE_SPEND").expect("LIVE_SPEND");
    let view = std::env::var("LIVE_VIEW").expect("LIVE_VIEW");
    let from: u64 = std::env::var("LIVE_FROM").ok().and_then(|f| f.parse().ok()).expect("LIVE_FROM");
    let keys = Keys::from_spend_key(Network::Stagenet, &spend, Some(&view)).unwrap();
    let wallet = Wallet::new(&dir("light-live-existing"), [1; 32], keys, Some(from)).unwrap();
    if let Ok(node) = std::env::var("LIVE_NODE") {
        wallet.inner.state.lock().await.node = node;
    }
    println!("address {}", wallet.primary_address());
    wallet.set_light_server(&url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    for poll in 1..=12 {
        let result = wallet.sync_step().await;
        live_report(&wallet, &format!("poll {poll}: {result:?}")).await;
        assert!(result.is_ok(), "nothing refused: {result:?}");
        if wallet.status().synced {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let status = wallet.status();
    assert!(status.synced);
    println!("light server screen: {:?}", wallet.light_standing().await);
    {
        let state = wallet.inner.state.lock().await;
        println!("node vouched for {:?}, kept near the tip {:?}", state.light_blocks, state.light_history.last());
    }
    assert_eq!(status.light.as_ref().unwrap().amounts, catch_up::Amounts::Chain, "every output proved, in a block the node has");
    let state = wallet.inner.state.lock().await;
    assert!(!state.outputs.is_empty());
    assert!(state.spends.iter().all(|s| s.fee > 0), "every spend with its fee");
    drop(state);
    println!("balance {:?}", wallet.balance().await);
    for day in wallet.said_lists(clock_now()).await.days {
        println!("{day:?}");
    }
}

/// The wallet in `LIVE_SPEND`/`LIVE_VIEW` (a stagenet test wallet), caught up
/// by the node and then synced through the server at `LWS_URL`, polling for
/// `LIVE_WAIT` seconds (default 60) so a payment sent to it meanwhile shows
/// in the pool and then mined.
#[tokio::test]
#[ignore = "needs monokulo-lws on stagenet at LWS_URL, a test wallet's keys, and the network"]
async fn live_a_payment_through_monokulo_lws() {
    let url = std::env::var("LWS_URL").unwrap_or_else(|_| "http://127.0.0.1:18443".into());
    let spend = std::env::var("LIVE_SPEND").expect("LIVE_SPEND");
    let view = std::env::var("LIVE_VIEW").expect("LIVE_VIEW");
    let wait: u64 = std::env::var("LIVE_WAIT").ok().and_then(|w| w.parse().ok()).unwrap_or(60);
    let keys = Keys::from_spend_key(Network::Stagenet, &spend, Some(&view)).unwrap();
    // Caught up by the node first, from a few blocks back.
    let probe = Wallet::create(&dir("light-live-probe"), [1; 32], Network::Stagenet).unwrap();
    while let Err(e) = probe.sync_step().await {
        println!("probe failed, again: {e}");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let tip = probe.inner.state.lock().await.tip;
    let wallet = Wallet::new(&dir("light-live-pay"), [1; 32], keys, Some(tip - 100)).unwrap();
    println!("address {}", wallet.primary_address());
    // Public nodes drop connections now and then: a failed step is tried again.
    loop {
        match wallet.sync_step().await {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                println!("node step failed, again: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    live_report(&wallet, "node caught up").await;
    wallet.set_light_server(&url, false).await.unwrap();
    wallet.use_light_wallet(true).await.unwrap();
    let started = Instant::now();
    let mut poll = 0;
    while started.elapsed() < Duration::from_secs(wait) {
        poll += 1;
        let result = wallet.sync_step().await;
        live_report(&wallet, &format!("poll {poll}: {result:?}")).await;
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    let balance = wallet.balance().await;
    println!("balance {balance:?}");
    let scanned = wallet.inner.state.lock().await.light_scanned_height;
    wallet.use_light_wallet(false).await.unwrap();
    live_report(&wallet, &format!("light off, server had scanned {scanned}")).await;
    let more = wallet.sync_step().await;
    live_report(&wallet, &format!("node step: {more:?}")).await;
    for day in wallet.said_lists(clock_now()).await.days {
        println!("{day:?}");
    }
}

mod send;

mod trezor;

/// A payment to another account (an app's, decisions 418 and 419) is found
/// the node way and the light way alike, kept in its account, and moves
/// account 0's Receive on not at all; the light server is told of the
/// account's addresses first. Unwatched, it isn't found.
#[tokio::test]
async fn a_payment_to_another_account_is_found_both_ways() {
    let wallet = Wallet::create(&dir("other-account"), [1; 32], Network::Stagenet).unwrap();
    let keys = &wallet.inner.keys;
    let (t1, h1, paid) = transaction_at(keys, Some((XMR, 3, 7)), &[], Timelock::None);
    let paid = paid.unwrap();
    let (b1, _, _) = block(keys, 300_101, None, vec![(t1, h1)]);
    let hash = b1.block.hash();
    let scan_with = |watch: &keyed::Watch| {
        let (scanner, prefilter) = wallet.scanner(watch);
        scan::scan(&scanner, &prefilter, vec![b1.clone()], &[hash], false).0
    };

    // Account 3 not watched yet: nothing found.
    let results = scan_with(&keyed::Watch::first(Wallet::watched(0)));
    assert!(results.into_iter().all(|r| r.unwrap().found.is_empty()));

    // The receive account set, its pool is watched and the payment found.
    wallet.set_receive_account(3).await.unwrap();
    let plan = wallet.inner.state.lock().await.watch_plan();
    let node_state = {
        let mut state = wallet.inner.state.lock().await;
        let mut images = Wallet::key_images(&state);
        for result in scan_with(&plan) {
            wallet.take_block(&mut state, &mut images, &HashMap::new(), result.unwrap());
        }
        assert_eq!(state.outputs.len(), 1);
        assert_eq!((state.outputs[0].account, state.outputs[0].subaddress, state.outputs[0].amount), (3, 7, XMR));
        assert_eq!(state.receive_index, 0, "account 0's Receive doesn't move");
        assert_eq!(state.found_in.get(&3), Some(&7));
        let kept = serde_json::to_value(&state.outputs).unwrap();
        state.outputs.clear();
        state.found_in.clear();
        state.restore_height = Some(300_100);
        state.next_block = 300_102;
        kept
    };

    // The light way.
    let listed = vec![listed(h1, Some(300_101), vec![received(&paid, 3_001_011)], vec![])];
    let txs = serde_json::to_string(&answer(listed, 300_299)).unwrap();
    let told = Arc::new(std::sync::Mutex::new(vec![]));
    let heard = told.clone();
    let url = scripted_node(move |text| {
        let txs = txs.clone();
        let heard = heard.clone();
        async move {
            if text.starts_with("POST /login") {
                r#"{"new_address":true,"start_height":300100}"#.to_owned()
            } else if text.starts_with("POST /upsert_subaddrs") {
                heard.lock().unwrap().push(text.clone());
                r#"{"new_subaddrs":[]}"#.to_owned()
            } else if text.starts_with("POST /get_address_txs") {
                txs
            } else {
                "{}".to_owned()
            }
        }
    })
    .await;
    {
        let mut state = wallet.inner.state.lock().await;
        state.light_url = Some(url);
        state.sync_mode = SyncMode::Light;
    }
    assert!(!wallet.sync_step().await.unwrap(), "one poll and done");
    let state = wallet.inner.state.lock().await;
    assert_eq!(serde_json::to_value(&state.outputs).unwrap(), node_state, "the same output, byte for byte");
    let upsert = told.lock().unwrap().concat();
    assert!(upsert.contains(r#"{"key":3,"value":[[0,60]]}"#), "told of account 3's pool: {upsert}");
    assert_eq!(state.light_accounts_sent, vec![(3, keyed::POOL_SIZE + keyed::POOL_MARGIN)]);
}
