//! The scan, over blocks made to pay a wallet. The prefilter's own tests, held to
//! curve25519-dalek and to monero-oxide's `Scanner`, are in monokulo-8ar.

use curve25519_dalek::{EdwardsPoint, Scalar, constants::ED25519_BASEPOINT_TABLE};
use monero_wallet::{
    Scanner, ViewPair, WalletOutput,
    address::{Network, SubaddressIndex},
    block::{Block, BlockHeader},
    ed25519::{CompressedPoint, Point, Scalar as MoneroScalar},
    interface::ScannableBlock,
    ringct::{EncryptedAmount, PrunedRctProofs, RctBase, RctType},
    transaction::{Input, Output, Pruned, Timelock, Transaction, TransactionPrefix},
};
use rand::{Rng, RngCore, SeedableRng, rngs::StdRng};
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

use crate::{Prefilter, Txs, scan};

fn varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push(n as u8 | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn random_scalar(rng: &mut StdRng) -> Scalar {
    let mut bytes = [0; 64];
    rng.fill_bytes(&mut bytes);
    Scalar::from_bytes_mod_order_wide(&bytes)
}

fn dalek_derivation(view: &Scalar, r: &EdwardsPoint) -> [u8; 32] {
    (view * r).mul_by_cofactor().compress().to_bytes()
}

struct Wallet {
    view: Scalar,
    spend: EdwardsPoint,
    pair: ViewPair,
}

const SUBADDRESS: u32 = 5;

fn wallet() -> (Wallet, Prefilter, Scanner) {
    let view_bytes = Zeroizing::new([9u8; 32]);
    let view_monero = MoneroScalar::read(&mut view_bytes.as_slice()).unwrap();
    let view: Scalar = view_monero.into();
    let spend = &Scalar::from(7u64) * ED25519_BASEPOINT_TABLE;
    let pair = ViewPair::new(Point::from(spend), Zeroizing::new(view_monero)).unwrap();
    let mut filter = Prefilter::new(&pair, &view_bytes);
    let mut scanner = Scanner::new(pair.clone());
    let index = SubaddressIndex::new(0, SUBADDRESS).unwrap();
    filter.watch(&pair, index);
    scanner.register_subaddress(index);
    (Wallet { view, spend, pair }, filter, scanner)
}

/// An output of the transaction with key `r`, at `index`, paying `to`.
fn paying(wallet: &Wallet, r: &Scalar, index: usize, to: EdwardsPoint) -> Output {
    let derivation = dalek_derivation(&wallet.view, &(r * ED25519_BASEPOINT_TABLE));
    let mut tagged = b"view_tag".to_vec();
    tagged.extend_from_slice(&derivation);
    varint(index, &mut tagged);
    let mut shared = derivation.to_vec();
    varint(index, &mut shared);
    let shared = Scalar::from_bytes_mod_order(Keccak256::digest(&shared).into());
    let key = &shared * ED25519_BASEPOINT_TABLE + to;
    Output { amount: Some(1_000), key: CompressedPoint::from(key.compress().to_bytes()), view_tag: Some(Keccak256::digest(&tagged)[0]) }
}

fn stranger(rng: &mut StdRng) -> Output {
    let key = &random_scalar(rng) * ED25519_BASEPOINT_TABLE;
    Output { amount: Some(1_000), key: CompressedPoint::from(key.compress().to_bytes()), view_tag: Some(rng.r#gen()) }
}

fn extra_of(keys: &[[u8; 32]], additional: Option<&[[u8; 32]]>) -> Vec<u8> {
    let mut extra = vec![];
    for key in keys {
        extra.push(1);
        extra.extend_from_slice(key);
    }
    if let Some(list) = additional {
        extra.push(4);
        extra.push(list.len() as u8);
        for key in list {
            extra.extend_from_slice(key);
        }
    }
    extra
}

fn point_of(r: &Scalar) -> [u8; 32] {
    (r * ED25519_BASEPOINT_TABLE).compress().to_bytes()
}

fn block_of(outputs: Vec<Output>, extra: Vec<u8>) -> ScannableBlock {
    block_with(outputs, extra, 16, 0)
}

fn block_with(outputs: Vec<Output>, extra: Vec<u8>, fork: u8, version: u8) -> ScannableBlock {
    let prefix = TransactionPrefix { additional_timelock: Timelock::None, inputs: vec![Input::Gen(100)], outputs, extra };
    let miner = if version == 1 { Transaction::V1 { prefix, signatures: vec![] } } else { Transaction::V2 { prefix, proofs: None } };
    let header = BlockHeader { hardfork_version: fork, hardfork_signal: fork, timestamp: 1, previous: [0; 32], nonce: 0 };
    ScannableBlock { block: Block::new(header, miner, vec![]).unwrap(), transactions: vec![], output_index_for_first_ringct_output: Some(0) }
}

/// On one thread and on several: each block's answer is its own. Without the GPU
/// (not allowed here, as for a wallet whose check hasn't passed) nothing is
/// filtered, so every block has its money looked for.
#[test]
fn a_scan_finds_money_in_the_block_that_holds_it() {
    let (w, filter, scanner) = wallet();
    let mut rng = StdRng::seed_from_u64(9);
    for count in [3usize, 40] {
        let pays = |i: usize| i % 13 == 1;
        let blocks: Vec<_> = (0..count)
            .map(|i| {
                let r = random_scalar(&mut rng);
                let output = if pays(i) { paying(&w, &r, 0, w.spend) } else { stranger(&mut rng) };
                block_of(vec![output], extra_of(&[point_of(&r)], None))
            })
            .collect();
        let hashes: Vec<[u8; 32]> = blocks.iter().map(|b| b.block.hash()).collect();
        let miners: Vec<String> = blocks.iter().map(|b| hex::encode(b.block.miner_transaction().hash())).collect();
        let (results, last) = scan::scan(&scanner, &filter, blocks, &hashes, false);
        assert_eq!(results.len(), count);
        assert!(last.is_some());
        for (i, result) in results.into_iter().enumerate() {
            let scanned = result.unwrap();
            assert_eq!(scanned.hash, hex::encode(hashes[i]), "block {i} of {count}");
            assert_eq!(!scanned.found.is_empty(), pays(i), "block {i} of {count}");
            assert_eq!(scanned.miner_tx, miners[i], "block {i} of {count}: read");
        }
    }
}

/// A transaction of `outputs` and `extra`.
fn tx_of(version: u8, outputs: Vec<Output>, extra: Vec<u8>) -> Transaction<Pruned> {
    // Two inputs of sixteen ring members, as a mainnet transaction has.
    let input = |rng: u64| Input::ToKey { amount: None, key_offsets: (0..16).map(|i| rng * 16 + i).collect(), key_image: CompressedPoint::from(point_of(&Scalar::from(rng + 1))) };
    let inputs = vec![input(1), input(2)];
    let n = outputs.len();
    let prefix = TransactionPrefix { additional_timelock: Timelock::None, inputs, outputs, extra };
    if version == 1 {
        Transaction::V1 { prefix, signatures: () }
    } else {
        let base = RctBase { fee: 30_000_000, pseudo_outs: vec![], encrypted_amounts: vec![EncryptedAmount::Compact { amount: [7; 8] }; n], commitments: vec![CompressedPoint::from(point_of(&Scalar::from(3u8))); n] };
        Transaction::V2 { prefix, proofs: Some(PrunedRctProofs { rct_type: RctType::ClsagBulletproofPlus, base }) }
    }
}

/// A block whose miner transaction has `miner` and whose others are
/// `transactions`, its first RingCT output at `first_index`.
fn block_of_txs(miner: (Vec<Output>, Vec<u8>), transactions: Vec<Transaction<Pruned>>, first_index: u64) -> ScannableBlock {
    let mut block = block_of(miner.0, miner.1);
    let hashes = (0..transactions.len()).map(|t| Keccak256::digest(format!("tx {t}")).into()).collect();
    block.block = Block::new(block.block.header.clone(), block.block.miner_transaction().clone(), hashes).unwrap();
    block.transactions = transactions;
    block.output_index_for_first_ringct_output = Some(first_index);
    block
}

/// Everything the `Scanner` says of each output it finds, its global index included.
fn found(scanner: &Scanner, block: ScannableBlock) -> Vec<Vec<u8>> {
    scanner.clone().scan(block).unwrap().ignore_additional_timelock().iter().map(WalletOutput::serialize).collect()
}

/// Ten transactions after the miner's, of one to four outputs, one of them v1:
/// the third pays the wallet at its third output, the seventh a subaddress
/// through an additional key, and the miner transaction too if `miner_pays`.
fn several_paying(w: &Wallet, miner_pays: bool) -> ScannableBlock {
    let mut rng = StdRng::seed_from_u64(if miner_pays { 21 } else { 20 });
    let sub = sub_spend(w);
    let r = random_scalar(&mut rng);
    let miner = if miner_pays { vec![stranger(&mut rng), paying(w, &r, 1, w.spend)] } else { vec![stranger(&mut rng)] };
    let mut transactions = vec![];
    for t in 1..=10 {
        let r = random_scalar(&mut rng);
        let mut outputs: Vec<Output> = (0..(t % 4 + 1)).map(|_| stranger(&mut rng)).collect();
        let mut extra = extra_of(&[point_of(&r)], None);
        match t {
            3 => outputs[2] = paying(w, &r, 2, w.spend),
            7 => {
                let other = point_of(&random_scalar(&mut rng));
                outputs[1] = paying(w, &r, 1, sub);
                extra = extra_of(&[other], Some(&vec![point_of(&r); outputs.len()]));
            }
            _ => {}
        }
        transactions.push(tx_of(if t == 5 { 1 } else { 2 }, outputs, extra));
    }
    block_of_txs((miner, extra_of(&[point_of(&r)], None)), transactions, 87_000_000)
}

fn sub_spend(w: &Wallet) -> EdwardsPoint {
    w.pair.subaddress(Network::Mainnet, SubaddressIndex::new(0, SUBADDRESS).unwrap()).spend().into()
}

/// The `Scanner` finds exactly what it finds in the whole block, global indexes
/// and all, in the transactions the prefilter flags: with the miner transaction
/// kept or left out, with transactions left out before, between and after the
/// ones kept, and with others kept beside them, as a block of nothing but
/// misses could be.
#[test]
fn the_flagged_transactions_hold_what_the_whole_block_does() {
    let (w, _, scanner) = wallet();
    for miner_pays in [false, true] {
        let block = several_paying(&w, miner_pays);
        let whole = found(&scanner, block.clone());
        assert_eq!(whole.len(), 2 + usize::from(miner_pays), "the block pays where it's meant to");
        let paying: Vec<usize> = if miner_pays { vec![0, 3, 7] } else { vec![3, 7] };
        let mut choices = vec![paying.clone()];
        for extra in [vec![0], vec![1], vec![10], vec![4, 5, 6], vec![2, 8], vec![1, 2, 4, 5, 6, 8, 9, 10], vec![0, 5]] {
            let mut txs = paying.clone();
            txs.extend(extra);
            txs.sort_unstable();
            txs.dedup();
            choices.push(txs);
        }
        for txs in choices {
            let part = scan::flagged_part(&block, &Txs::These(txs.clone()));
            assert!(part.transactions.len() < block.transactions.len() || txs.len() > 8, "{txs:?}: transactions are left out");
            assert_eq!(found(&scanner, part), whole, "miner pays: {miner_pays}, {txs:?}");
        }
        assert_eq!(found(&scanner, scan::flagged_part(&block, &Txs::All)), whole, "all of it");
        assert_eq!(scan::flagged_part(&block, &Txs::All).transactions.len(), block.transactions.len());
    }
}

/// Left out, a transaction holds nothing the `Scanner` sees: the test above
/// would pass for a part that kept everything.
#[test]
fn a_transaction_left_out_isnt_read() {
    let (w, _, scanner) = wallet();
    let block = several_paying(&w, true);
    let only_seventh = found(&scanner, scan::flagged_part(&block, &Txs::These(vec![7])));
    assert_eq!(only_seventh.len(), 1);
    assert!(found(&scanner, block).contains(&only_seventh[0]));
}

/// On the GPU, through `scan`: with the prefilter's own flags, on one thread
/// and several, every block's outputs are those the `Scanner` finds reading
/// every block whole.
#[test]
#[ignore = "needs a GPU; run with --ignored"]
fn on_the_gpu_a_scan_finds_what_reading_every_block_finds() {
    crate::warm_up();
    assert!(crate::settle(std::time::Duration::from_secs(120)) && crate::gpu_ready(), "the GPU started");
    let (w, filter, scanner) = wallet();
    let mut rng = StdRng::seed_from_u64(22);
    for count in [3usize, 40] {
        let blocks: Vec<ScannableBlock> = (0..count)
            .map(|i| match i % 5 {
                1 => several_paying(&w, false),
                3 => several_paying(&w, true),
                _ => {
                    let transactions = (0..6).map(|_| tx_of(2, vec![stranger(&mut rng), stranger(&mut rng)], extra_of(&[point_of(&random_scalar(&mut rng))], None))).collect();
                    block_of_txs((vec![stranger(&mut rng)], extra_of(&[point_of(&random_scalar(&mut rng))], None)), transactions, 1_000 * i as u64)
                }
            })
            .collect();
        let hashes: Vec<[u8; 32]> = blocks.iter().map(|b| b.block.hash()).collect();
        let flags = filter.flag_txs(&blocks, 1, 0).expect("the GPU filtered");
        assert_eq!(flags[1], Txs::These(vec![3, 7]), "the transactions that pay, by their place");
        assert_eq!(flags.get(3), (count > 3).then_some(&Txs::These(vec![0, 3, 7])));
        assert!(flags.iter().enumerate().all(|(i, txs)| txs.flagged() == (i % 5 == 1 || i % 5 == 3)), "{flags:?}");
        let (filtered, _) = scan::scan(&scanner, &filter, blocks.clone(), &hashes, true);
        let (unfiltered, _) = scan::scan(&scanner, &filter, blocks.clone(), &hashes, false);
        for (i, ((filtered, unfiltered), block)) in filtered.into_iter().zip(unfiltered).zip(blocks).enumerate() {
            let (filtered, unfiltered) = (filtered.unwrap(), unfiltered.unwrap());
            let outputs = |s: &scan::Scanned| s.found.iter().map(WalletOutput::serialize).collect::<Vec<_>>();
            assert_eq!(outputs(&filtered), outputs(&unfiltered), "block {i} of {count}");
            assert_eq!(outputs(&filtered), found(&scanner, block), "block {i} of {count}");
            assert_eq!(filtered.tx_keys, unfiltered.tx_keys, "block {i} of {count}");
            assert_eq!(filtered.found.is_empty(), i % 5 != 1 && i % 5 != 3, "block {i} of {count}");
        }
    }
}

/// What copying a block costs, whole and in the part the `Scanner` reads when
/// one transaction pays: `cargo test --release -p fob-scanner -- --ignored --nocapture copying`.
#[test]
#[ignore = "a measurement"]
fn copying_a_block() {
    let mut rng = StdRng::seed_from_u64(23);
    // About mainnet's: sixty transactions of two inputs and two outputs.
    let blocks: Vec<ScannableBlock> = (0..200)
        .map(|_| {
            let transactions = (0..60).map(|_| tx_of(2, vec![stranger(&mut rng), stranger(&mut rng)], extra_of(&[point_of(&random_scalar(&mut rng))], None))).collect();
            block_of_txs((vec![stranger(&mut rng)], extra_of(&[point_of(&random_scalar(&mut rng))], None)), transactions, 90_000_000)
        })
        .collect();
    let time = |what: &str, copy: &dyn Fn(&ScannableBlock) -> ScannableBlock| {
        let mut best = f64::MAX;
        for _ in 0..20 {
            let t = std::time::Instant::now();
            for block in &blocks {
                drop(std::hint::black_box(copy(block)));
            }
            best = best.min(t.elapsed().as_secs_f64());
        }
        println!("{what}: {:.2} µs a block", best * 1e6 / blocks.len() as f64);
    };
    time("the whole block", &|b| b.clone());
    time("one transaction", &|b| scan::flagged_part(b, &Txs::These(vec![31])));
    time("the miner's and one", &|b| scan::flagged_part(b, &Txs::These(vec![0, 31])));
    time("two apart", &|b| scan::flagged_part(b, &Txs::These(vec![12, 48])));
}
