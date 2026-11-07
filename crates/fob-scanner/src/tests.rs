//! The scan, over blocks made to pay a wallet. The prefilter's own tests, held to
//! curve25519-dalek and to monero-oxide's `Scanner`, are in monokulo-8ar.

use curve25519_dalek::{EdwardsPoint, Scalar, constants::ED25519_BASEPOINT_TABLE};
use monero_wallet::{
    Scanner, ViewPair,
    address::SubaddressIndex,
    block::{Block, BlockHeader},
    ed25519::{CompressedPoint, Point, Scalar as MoneroScalar},
    interface::ScannableBlock,
    transaction::{Input, Output, Timelock, Transaction, TransactionPrefix},
};
use rand::{Rng, RngCore, SeedableRng, rngs::StdRng};
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

use crate::{Prefilter, scan};

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
    (Wallet { view, spend }, filter, scanner)
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
