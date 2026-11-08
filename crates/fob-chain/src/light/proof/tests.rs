use monero_wallet::{
    block::BlockHeader,
    ed25519::{Commitment, CompressedPoint, Scalar},
    extra::ExtraField,
    ringct::{EncryptedAmount, PrunedRctProofs, RctBase, RctType},
    transaction::{Output, TransactionPrefix},
};

use super::*;

/// A point on the curve, different for each `i`.
fn point(i: u8) -> CompressedPoint {
    Commitment::new(Scalar::hash([i]), 0).commit().compress()
}

const HEIGHT: usize = 2_245_514;
const TIME: u64 = 1_793_990_000;

/// A block at [`HEIGHT`] holding one transaction (two outputs, fee 30,000,
/// key `point(1)`), its reward (600 XMR, key `point(8)`), and what an honest
/// server lists for the transaction's second output and the reward.
fn fixture() -> (Block, lws::Transaction, lws::Transaction) {
    let tx: Transaction<Pruned> = Transaction::V2 {
        prefix: TransactionPrefix {
            additional_timelock: Timelock::None,
            inputs: vec![Input::ToKey { amount: None, key_offsets: vec![1; 16], key_image: point(6) }],
            outputs: vec![Output { amount: None, key: point(2), view_tag: Some(1) }, Output { amount: None, key: point(3), view_tag: Some(2) }],
            extra: ExtraField::PublicKey(point(1)).serialize(),
        },
        proofs: Some(PrunedRctProofs {
            rct_type: RctType::ClsagBulletproofPlus,
            base: RctBase { fee: 30_000, pseudo_outs: vec![], encrypted_amounts: vec![EncryptedAmount::Compact { amount: [0; 8] }; 2], commitments: vec![point(4), point(5)] },
        }),
    };
    let prunable = [7; 32];
    let hash = tx.hash_with_prunable_hash(prunable).unwrap();
    let miner = Transaction::V2 {
        prefix: TransactionPrefix {
            additional_timelock: Timelock::Block(HEIGHT + 60),
            inputs: vec![Input::Gen(HEIGHT)],
            outputs: vec![Output { amount: Some(600_000_000_000), key: point(7), view_tag: Some(0) }],
            extra: ExtraField::PublicKey(point(8)).serialize(),
        },
        proofs: None,
    };
    let block = Block::new(BlockHeader { hardfork_version: 16, hardfork_signal: 16, timestamp: TIME, previous: [3; 32], nonce: 9 }, miner, vec![[1; 32], hash]).unwrap();
    let listed = |hash: [u8; 32], receive: lws::ReceivedOutput, proof: lws::Proof| lws::Transaction {
        id: 1,
        hash: lws::Hex(hash),
        timestamp: Some(lws::Timestamp::from_unix(TIME)),
        total_received: receive.amount,
        total_sent: lws::Amount(0),
        fee: Some(lws::Amount(30_000)),
        unlock_time: 0,
        height: Some(HEIGHT as u64),
        spent_outputs: vec![],
        receives: vec![receive],
        payment_id: None,
        coinbase: false,
        mempool: false,
        mixin: 15,
        proof: Some(proof),
    };
    let receive = |key: CompressedPoint, index: u16, tx_key: CompressedPoint, amount: u64, commitment: Option<CompressedPoint>| lws::ReceivedOutput {
        amount: lws::Amount(amount),
        public_key: lws::Hex(key.to_bytes()),
        index,
        global_index: 9,
        tx_pub_key: lws::Hex(tx_key.to_bytes()),
        rct: None,
        commitment: commitment.map(|c| lws::Hex(c.to_bytes())),
        recipient: lws::AddressMeta { maj_i: 0, min_i: 0 },
    };
    let paid = listed(hash, receive(point(3), 1, point(1), 5, Some(point(5))), make(&block, Some((&tx, prunable))));
    let mut reward = listed(block.miner_transaction().hash(), receive(point(7), 0, point(8), 600_000_000_000, None), make(&block, None));
    reward.coinbase = true;
    reward.fee = Some(lws::Amount(0));
    reward.unlock_time = (HEIGHT + 60) as u64;
    (block, paid, reward)
}

#[test]
fn an_honest_proof_gives_the_block() {
    let (block, paid, reward) = fixture();
    let proven = check(&paid).unwrap().unwrap();
    assert_eq!(proven, Proven { height: HEIGHT as u64, block: block.hash(), time: TIME, commitments: vec![Some(point(5).to_bytes())] });
    // Without a commitment from the server, the chain's comes back.
    let mut bare = paid.clone();
    bare.receives[0].commitment = None;
    assert_eq!(check(&bare).unwrap().unwrap().commitments, vec![Some(point(5).to_bytes())]);
    // A mined reward: the block's own miner transaction.
    let proven = check(&reward).unwrap().unwrap();
    assert_eq!((proven.block, proven.commitments), (block.hash(), vec![None]));
    // No proof: nothing to say.
    let mut none = paid;
    none.proof = None;
    assert_eq!(check(&none), Ok(None));
}

#[test]
fn a_proof_that_doesn_t_fit_is_broken() {
    let (_, paid, reward) = fixture();
    let broken = |change: &dyn Fn(&mut lws::Transaction)| {
        let mut tx = paid.clone();
        change(&mut tx);
        check(&tx).unwrap_err()
    };
    let proof = |tx: &mut lws::Transaction| tx.proof.as_mut().unwrap().clone();
    // A tampered prefix: another one-time key at index 1.
    let tampered = |tx: &mut lws::Transaction| {
        let mut p = proof(tx);
        let prefix = p.tx_prefix.as_mut().unwrap();
        let at = prefix.0.windows(32).position(|w| w == point(3).to_bytes()).unwrap();
        prefix.0[at..at + 32].copy_from_slice(&point(9).to_bytes());
        tx.proof = Some(p);
    };
    assert_eq!(broken(&tampered), Broken::Hash);
    assert_eq!(broken(&|tx| tx.hash = lws::Hex([5; 32])), Broken::Hash);
    assert_eq!(broken(&|tx| tx.proof.as_mut().unwrap().prunable_hash = Some(lws::Hex([8; 32]))), Broken::Hash);
    assert_eq!(broken(&|tx| tx.height = Some(HEIGHT as u64 + 1)), Broken::NotInBlock);
    assert_eq!(broken(&|tx| tx.receives[0].public_key = lws::Hex(point(2).to_bytes())), Broken::Output);
    assert_eq!(broken(&|tx| tx.receives[0].index = 2), Broken::Output);
    assert_eq!(broken(&|tx| tx.receives[0].commitment = Some(lws::Hex(point(4).to_bytes()))), Broken::Output);
    assert_eq!(broken(&|tx| tx.receives[0].tx_pub_key = lws::Hex(point(9).to_bytes())), Broken::TxKey);
    assert_eq!(broken(&|tx| tx.fee = Some(lws::Amount(1))), Broken::Terms);
    assert_eq!(broken(&|tx| tx.unlock_time = 10), Broken::Terms);
    assert_eq!(broken(&|tx| tx.timestamp = Some(lws::Timestamp::from_unix(TIME + 1))), Broken::Terms);
    assert_eq!(broken(&|tx| tx.proof.as_mut().unwrap().block.0.push(0)), Broken::Unreadable);
    assert_eq!(broken(&|tx| tx.proof.as_mut().unwrap().rct_base = None), Broken::Unreadable);
    assert_eq!(broken(&|tx| tx.coinbase = true), Broken::NotInBlock);
    // A reward claimed that isn't the block's.
    let mut fake = reward.clone();
    fake.hash = lws::Hex([4; 32]);
    assert_eq!(check(&fake).unwrap_err(), Broken::NotInBlock);
    let mut wrong_amount = reward;
    wrong_amount.receives[0].amount = lws::Amount(1);
    assert_eq!(check(&wrong_amount).unwrap_err(), Broken::Output);
}
