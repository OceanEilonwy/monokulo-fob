//! Syncing through a light wallet server: `reconcile`, the `WalletOutput`
//! bytes, the migration, the setting, and the light path against the node
//! path on the same blocks.

use super::*;

use crate::light::lws;
use curve25519_dalek::{EdwardsPoint, Scalar as DScalar, constants::ED25519_BASEPOINT_TABLE};
use crate::wallet::light::{WalletOutputParts, reconcile, unix_time, wallet_output_bytes};
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
}

/// An output paying `amount` to `keys`' subaddress `sub` (0: the main
/// address) at `index`, from a fresh transaction key: the key, the view tag,
/// the transaction key, the mask, the encrypted amount.
fn pay(keys: &Keys, amount: u64, sub: u32, index: u16, coinbase: bool) -> (Output, Paid, Option<(Commitment, [u8; 8])>) {
    let spend: EdwardsPoint = match SubaddressIndex::new(0, sub) {
        Some(s) => keys.view_pair().subaddress(keys.network.monero(), s).spend().into(),
        None => keys.view_pair().spend().into(),
    };
    let r = random_scalar();
    // To a subaddress the transaction key is r·D, to the main address r·G.
    let big_r = if sub == 0 { &r * ED25519_BASEPOINT_TABLE } else { spend * r };
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
    let paid = Paid { index, key, tx_key: big_r.compress().to_bytes(), mask: <[u8; 32]>::from(mask), amount, subaddress: sub };
    (output, paid, encrypted)
}

fn stray_point() -> CompressedPoint {
    CompressedPoint::from((&random_scalar() * ED25519_BASEPOINT_TABLE).compress().to_bytes())
}

/// A transaction paying `amount` to `sub` (or nobody, with `None`), spending
/// `images`, with a fee of 1,000.
fn transaction(keys: &Keys, pays: Option<(u64, u32)>, images: &[[u8; 32]]) -> (Transaction<Pruned>, [u8; 32], Option<Paid>) {
    let (output, paid, commitment, encrypted, tx_key) = match pays {
        Some((amount, sub)) => {
            let (output, paid, enc) = pay(keys, amount, sub, 0, false);
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
        prefix: TransactionPrefix { additional_timelock: Timelock::None, inputs, outputs: vec![output], extra: ExtraField::PublicKey(CompressedPoint::from(tx_key)).serialize() },
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
        recipient: lws::AddressMeta { maj_i: 0, min_i: paid.subaddress },
    }
}

fn listed(hash: [u8; 32], height: Option<u64>, receives: Vec<lws::ReceivedOutput>, spent_outputs: Vec<lws::Spend>) -> lws::Transaction {
    lws::Transaction {
        id: 1,
        hash: lws::Hex(hash),
        timestamp: height.map(|h| lws::Timestamp::from_unix(1_700_000_000 + h * 120)),
        total_received: lws::Amount(receives.iter().map(|r| r.amount.0).sum()),
        total_sent: lws::Amount(spent_outputs.iter().map(|s| s.amount.0).sum()),
        unlock_time: 0,
        height,
        spent_outputs,
        receives,
        payment_id: None,
        coinbase: false,
        mempool: height.is_none(),
        mixin: 15,
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
    let (offset, _) = keys.light_output(&paid.tx_key, paid.index.into(), paid.subaddress, &paid.key).unwrap();
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
    let spend = listed(h3, Some(300_103), vec![], vec![spend_of(&p1, image), spend_of(&p2, [9; 32])]);
    Chain {
        blocks: [b1, b2, b3].into_iter().map(|b| { let hash = b.block.hash(); (b, hash) }).collect(),
        listed: vec![coinbase, listed(h1, Some(300_101), vec![received(&p1, 3_001_011)], vec![]), listed(h2, Some(300_102), vec![received(&p2, 3_001_021)], vec![]), spend],
    }
}

/// What a state holds that both paths must agree on.
fn found(state: &State) -> (serde_json::Value, Vec<(String, u64, u64)>, u64, u64, u32) {
    let outputs = serde_json::to_value(&state.outputs).unwrap();
    let spends = state.spends.iter().map(|s| (s.tx.clone(), s.height, s.time)).collect();
    let total = state.outputs.iter().filter(|o| o.spent_in.is_none()).map(|o| o.amount).sum();
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
    let (scanner, prefilter) = wallet.scanner(Wallet::watched(0));
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
    assert_eq!(light_state.1, node_state.1, "the same spends");
    assert_eq!((light_state.2, light_state.3, light_state.4), (node_state.2, node_state.3, node_state.4), "the same balance and addresses");
    assert!(state.spends.iter().all(|s| s.fee == 0), "the API gives no fee");
    assert!(state.light_registered);
    assert_eq!(state.light_subaddrs_sent, Wallet::watched(0), "told of the addresses watched before the poll");
    assert_eq!((state.light_scanned_height, state.tip), (300_299, 300_300));
    drop(state);
    let status = wallet.status();
    assert!(status.synced && status.connected);
    assert_eq!(fob_words::catch_up::chip(status.chip()).text, "Synced · server");
}

/// The bytes Fob writes for a `WalletOutput` are those monero-wallet 0.2's
/// scanner makes, for the main address, a subaddress and a mined reward.
#[test]
fn wallet_output_bytes_are_monero_wallet_s() {
    let keys = Keys::generate(Network::Stagenet).keys;
    let chain = chain(&keys);
    let mut scanner = Scanner::new(keys.view_pair());
    Wallet::watch(&mut scanner, 1, 10);
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
    let state = wallet.inner.state.try_lock().unwrap().clone();
    (wallet, state)
}

#[test]
fn reconcile_receives_spends_and_is_idempotent() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    let response = answer(chain.listed.clone(), 400_000);
    let first = reconcile(keys, &mut state, &response, &HashMap::new(), 5);
    assert!(first.changed);
    assert_eq!(first.refused, 0);
    assert_eq!(state.outputs.len(), 3);
    let spent: Vec<_> = state.outputs.iter().filter(|o| o.spent_in.is_some()).collect();
    assert_eq!(spent.len(), 1, "the real spend only, not the decoy use");
    assert_eq!(spent[0].amount, 3 * XMR);
    assert_eq!(state.receive_index, 3);
    let coinbase = state.outputs.iter().find(|o| o.coinbase).unwrap();
    assert_eq!(coinbase.unlock_block, 300_161);
    let saved = serde_json::to_value(&state).unwrap();
    let again = reconcile(keys, &mut state, &response, &HashMap::new(), 6);
    assert!(!again.changed, "the same answer twice changes nothing");
    assert_eq!(serde_json::to_value(&state).unwrap(), saved);
}

#[test]
fn a_transaction_the_server_drops_was_reorganised_out() {
    let (wallet, mut state) = fresh();
    let keys = &wallet.inner.keys;
    let chain = chain(keys);
    reconcile(keys, &mut state, &answer(chain.listed.clone(), 400_000), &HashMap::new(), 5);
    state.sent.push(Sent { height: Some(300_103), ..sent_payment(&hex::encode(chain.listed[3].hash.0)) });
    // The spend in block 103 is gone from the chain: its coin is free again,
    // the payment unmined; what's below stays.
    let shorter: Vec<_> = chain.listed[..3].to_vec();
    let done = reconcile(keys, &mut state, &answer(shorter.clone(), 400_000), &HashMap::new(), 6);
    assert!(done.changed);
    assert!(state.spends.is_empty());
    assert_eq!(state.sent[0].height, None);
    // Its coin stays spent: this wallet's own payment, now waiting, holds it.
    assert_eq!(state.outputs.iter().filter(|o| o.spent_in.is_some()).count(), 1);
    // The payment to subaddress 3 goes too: its output is dropped.
    let done = reconcile(keys, &mut state, &answer(shorter[..2].to_vec(), 400_000), &HashMap::new(), 7);
    assert!(done.changed);
    assert_eq!(state.outputs.len(), 2);
    // A server that hasn't scanned that far (after a request to scan from
    // the start) vouches for nothing past its height: nothing more goes.
    let done = reconcile(keys, &mut state, &answer(vec![], 50), &HashMap::new(), 8);
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
        receipt: None,
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
    let done = reconcile(keys, &mut state, &answer(vec![pooled], 200), &HashMap::new(), 42);
    assert!(state.outputs.is_empty(), "not owned until mined");
    assert_eq!(done.pool.get(&hex::encode(hash)), Some(&Pooled { seen: 42, parts: vec![(2, XMR)] }));
    // Seen again later: still when it was first seen.
    let pooled = listed(hash, None, vec![received(&paid, 0)], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![pooled], 200), &done.pool, 99);
    assert_eq!(done.pool[&hex::encode(hash)].seen, 42);
    // Mined: owned, and out of the pool.
    let mined = listed(hash, Some(150), vec![received(&paid, 1500)], vec![]);
    let done = reconcile(keys, &mut state, &answer(vec![mined], 200), &done.pool, 120);
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
    forged.public_key = lws::Hex(stray_point().to_bytes());
    // The right key with another mask: a commitment that isn't on the chain.
    let mut wrong_mask = received(&paid, 8);
    wrong_mask.public_key = lws::Hex(paid.key);
    wrong_mask.rct = Some(lws::Hex(<[u8; 32]>::from(Scalar::hash(b"another"))));
    let done = reconcile(keys, &mut state, &answer(vec![listed(hash, Some(300_000), vec![forged.clone(), wrong_mask.clone()], vec![])], 400_000), &HashMap::new(), 1);
    assert_eq!(done.refused, 2);
    assert!(state.outputs.is_empty());
    // Before hard fork 10 a mask came encrypted, not from the view key: it is
    // taken on the server's word (a wrong one only makes the coin unspendable).
    let done = reconcile(keys, &mut state, &answer(vec![listed(hash, Some(150), vec![forged, wrong_mask], vec![])], 400_000), &HashMap::new(), 1);
    assert_eq!((done.refused, state.outputs.len()), (1, 1));
    state.outputs.clear();
    // A time lock: dropped, as decision 48 has it.
    let mut locked = listed(hash, Some(150), vec![received(&paid, 9)], vec![]);
    locked.unlock_time = 1_900_000_000;
    let done = reconcile(keys, &mut state, &answer(vec![locked], 200), &HashMap::new(), 1);
    assert_eq!((done.refused, state.outputs.len()), (0, 0));
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
        "PRAGMA key = \"x'{}'\"; ALTER TABLE wallet DROP COLUMN sync_mode; ALTER TABLE wallet DROP COLUMN light_url; ALTER TABLE wallet DROP COLUMN light_self_signed; ALTER TABLE wallet DROP COLUMN light_pin; ALTER TABLE wallet DROP COLUMN light_registered; ALTER TABLE wallet DROP COLUMN light_subaddrs_sent; ALTER TABLE wallet DROP COLUMN light_scanned_height; PRAGMA user_version = 7;",
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
/// another network or onion; on, ZMQ is off and Manual Sync moot, Send says
/// it comes next; off, node scanning starts from the server's height less 30.
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
    assert!(matches!(wallet.prepare(&wallet.primary_address(), 1, None, None).await, Err(FobError::SendingThroughServerLater)));
    assert_eq!(FobError::SendingThroughServerLater.to_string(), "Sending through a server comes next");

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

