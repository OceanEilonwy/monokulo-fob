//! Signing a payment with the device, step by step as Monero's
//! cold-signing protocol goes (`device_trezor.cpp`, `protocol.cpp`):
//!
//! 1. `Init`: the whole payment as the device shows it (destinations,
//!    amounts, fee). It answers with an HMAC per destination.
//! 2. `SetInput` for each input: the device makes its key image, `vin`,
//!    and the pseudo output commitment, sealing what only it may open.
//! 3. The inputs are sorted by key image, as Monero orders them, and each
//!    goes back (`InputVini`) for the device to hash into the prefix.
//! 4. `SetOutput` for each destination: the device makes the one-time
//!    address, the commitment and the encrypted amount. With two outputs
//!    it makes the range proof too; with more it hands Fob the masks and
//!    takes the proof Fob makes.
//! 5. `AllOutSet`: the device gives the extra and the hashes it signs.
//!    Fob builds the same transaction and checks both hashes match.
//! 6. `SignInput` for each input: a CLSAG, sealed until `Final` hands over
//!    the key that opens them all, once nothing has gone wrong.
//!
//! Fob checks every signature and the range proof before the transaction
//! leaves, so a device (or a cable) that lied gives a payment that's
//! refused here, not one that burns money.

use curve25519_dalek::Scalar as DalekScalar;
use monero_wallet::{
    OutputWithDecoys,
    address::MoneroAddress,
    ed25519::{Commitment, CompressedPoint, Scalar},
    ringct::{EncryptedAmount, RctBase, RctProofs, RctPrunable, bulletproofs::Bulletproof, clsag::Clsag},
    transaction::{Input, Output, Timelock, Transaction, TransactionPrefix},
};
use rand_core::{CryptoRng, RngCore};

use super::messages::{self as m, Destination, RingMember, Source};
use super::{Session, crypto};
use crate::error::{FobError, Result};
use crate::network::Network;

/// An input to spend, with what the device needs to find its key.
#[derive(Clone)]
pub struct SpendInput {
    pub with_decoys: OutputWithDecoys,
    /// The public key its transaction carried, and the extra one at its
    /// index if the transaction had those.
    pub tx_pub_key: [u8; 32],
    pub additional: Option<[u8; 32]>,
    pub index_in_tx: u64,
    pub subaddress_minor: u32,
}

/// Weight added to the fee's for the device's transaction, which may carry a
/// few bytes more of extra than the one the fee was worked out for.
pub const WEIGHT_MARGIN: u64 = 48;

/// A payment to be signed.
#[derive(Clone)]
pub struct Plan {
    pub network: Network,
    pub inputs: Vec<SpendInput>,
    pub payments: Vec<(MoneroAddress, u64)>,
    /// What comes back to the wallet's primary address.
    pub change: u64,
    pub change_address: MoneroAddress,
    pub fee: u64,
    pub hard_fork: u32,
}

fn failed(text: impl Into<String>) -> FobError {
    FobError::Trezor(text.into())
}

fn destination(address: &MoneroAddress, amount: u64) -> Destination {
    Destination {
        amount,
        spend_public_key: address.spend().compress().to_bytes(),
        view_public_key: address.view().compress().to_bytes(),
        is_subaddress: address.is_subaddress(),
        is_integrated: address.payment_id().is_some(),
    }
}

fn source(input: &SpendInput) -> Source {
    let decoys = input.with_decoys.decoys();
    let ring = decoys
        .positions()
        .into_iter()
        .zip(decoys.ring())
        .map(|(index, member)| RingMember { index, key: member[0].compress().to_bytes(), commitment: member[1].compress().to_bytes() })
        .collect();
    let commitment = input.with_decoys.commitment();
    Source {
        ring,
        real_output: u64::from(decoys.signer_index()),
        tx_pub_key: input.tx_pub_key,
        additional: input.additional,
        index_in_tx: input.index_in_tx,
        amount: commitment.amount,
        mask: <[u8; 32]>::from(commitment.mask),
        subaddress_minor: input.subaddress_minor,
    }
}

/// The destinations as the device sees them: the payments, the change,
/// and a nothing-output in place of change of nothing (Monero's two-output
/// rule), in a random order as wallets send them.
fn outputs(plan: &Plan, rng: &mut (impl RngCore + CryptoRng)) -> Result<(Vec<Destination>, Destination)> {
    let change = destination(&plan.change_address, plan.change);
    let mut outputs: Vec<Destination> = plan.payments.iter().map(|(address, amount)| destination(address, *amount)).collect();
    if plan.change > 0 {
        outputs.push(change.clone());
    } else if outputs.len() == 1 {
        let mut random = [0u8; 32];
        rng.fill_bytes(&mut random);
        let random = (&DalekScalar::from_bytes_mod_order(random) * curve25519_dalek::constants::ED25519_BASEPOINT_TABLE).compress().to_bytes();
        outputs.push(Destination { amount: 0, spend_public_key: random, view_public_key: random, is_subaddress: false, is_integrated: false });
    } else {
        // The device wants the change among the outputs unless there are
        // exactly two: a split that leaves nothing over can't be signed.
        return Err(FobError::Send("this payment leaves no change at all, which a Trezor won't sign: change the amount a little".into()));
    }
    for i in (1..outputs.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        outputs.swap(i, j);
    }
    Ok((outputs, change))
}

/// A sum of amounts, or `None` if it's more than a `u64` holds.
fn checked_total(amounts: impl IntoIterator<Item = u64>) -> Option<u64> {
    amounts.into_iter().try_fold(0u64, u64::checked_add)
}

fn key_image_of(vini: &[u8]) -> Result<[u8; 32]> {
    vini.get(vini.len().saturating_sub(32)..).and_then(|k| k.try_into().ok()).ok_or_else(|| failed("the device's input is too short"))
}

fn compressed(bytes: &[u8]) -> Result<CompressedPoint> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| failed("the device gave a key of the wrong size"))?;
    Ok(CompressedPoint::from(bytes))
}

/// Runs the protocol with the device and brings back the signed
/// transaction, checked.
pub fn sign(session: &Session<'_>, plan: &Plan, view_key: &[u8; 32], rng: &mut (impl RngCore + CryptoRng)) -> Result<Transaction> {
    if plan.inputs.is_empty() {
        return Err(failed("nothing to spend"));
    }
    let (outputs, change) = outputs(plan, rng)?;
    // Checked: totals that wrapped round could seem to add up.
    let total_in = checked_total(plan.inputs.iter().map(|i| i.with_decoys.commitment().amount));
    let total_out = checked_total(outputs.iter().map(|o| o.amount).chain([plan.fee]));
    if total_in.is_none() || total_in != total_out {
        return Err(failed("the payment's amounts don't add up"));
    }
    let payment_id = plan.payments.iter().find_map(|(address, _)| address.payment_id());
    let integrated_indices = outputs.iter().enumerate().filter(|(_, o)| o.is_integrated).map(|(i, _)| i as u32).collect();
    let mut minors: Vec<u32> = plan.inputs.iter().map(|i| i.subaddress_minor).chain([0]).collect();
    minors.sort_unstable();
    minors.dedup();
    let ring_len = plan.inputs[0].with_decoys.decoys().len();
    let data = m::TransactionData {
        payment_id,
        outputs: outputs.clone(),
        change,
        num_inputs: plan.inputs.len() as u32,
        mixin: (ring_len - 1) as u32,
        fee: plan.fee,
        minor_indices: minors,
        integrated_indices,
        hard_fork: plan.hard_fork,
    };
    let steps = 3 + 3 * plan.inputs.len() as u32 + outputs.len() as u32;
    let mut step = 0;
    let mut progress = |session: &Session<'_>| {
        step += 1;
        session.prompts_progress(step, steps);
    };

    // 1. Init.
    let ack = session.expect(m::MONERO_TRANSACTION_INIT_REQUEST, &m::transaction_init(plan.network, &data), m::MONERO_TRANSACTION_INIT_ACK)?;
    let (hmacs, offload) = m::transaction_init_ack(&ack).map_err(|e| failed(e.to_string()))?;
    if hmacs.len() != outputs.len() {
        return Err(failed("the device gave the wrong number of output HMACs"));
    }
    progress(session);

    // 2. Inputs.
    let sources: Vec<Source> = plan.inputs.iter().map(source).collect();
    let mut acks = Vec::with_capacity(sources.len());
    for s in &sources {
        let ack = session.expect(m::MONERO_TRANSACTION_SET_INPUT_REQUEST, &m::set_input(s), m::MONERO_TRANSACTION_SET_INPUT_ACK)?;
        acks.push(m::set_input_ack(&ack).map_err(|e| failed(e.to_string()))?);
        progress(session);
    }

    // 3. Sorted by key image, largest first, as Monero orders inputs.
    let images: Vec<[u8; 32]> = acks.iter().map(|a| key_image_of(&a.vini)).collect::<Result<_>>()?;
    let mut order: Vec<usize> = (0..sources.len()).collect();
    order.sort_by(|a, b| images[*b].cmp(&images[*a]));
    for &original in &order {
        session.expect(m::MONERO_TRANSACTION_INPUT_VINI_REQUEST, &m::input_vini(&sources[original], &acks[original], original as u32), m::MONERO_TRANSACTION_INPUT_VINI_ACK)?;
        progress(session);
    }
    session.expect(m::MONERO_TRANSACTION_ALL_INPUTS_SET_REQUEST, &[], m::MONERO_TRANSACTION_ALL_INPUTS_SET_ACK)?;

    // 4. Outputs.
    let mut out_acks: Vec<m::OutputAck> = Vec::with_capacity(outputs.len());
    let mut bulletproof = None;
    for (i, output) in outputs.iter().enumerate() {
        let ack = session.expect(m::MONERO_TRANSACTION_SET_OUTPUT_REQUEST, &m::set_output(output, &hmacs[i], None), m::MONERO_TRANSACTION_SET_OUTPUT_ACK)?;
        let ack = m::set_output_ack(&ack).map_err(|e| failed(e.to_string()))?;
        if ack.out_pk.len() != 64 || ack.ecdh_info.len() != 8 {
            return Err(failed("the device's output has the wrong shape"));
        }
        out_acks.push(ack);
        let last = i + 1 == outputs.len();
        let commitments: Vec<CompressedPoint> = out_acks.iter().map(|a| compressed(&a.out_pk[32..])).collect::<Result<_>>()?;
        if offload {
            if !last {
                continue;
            }
            // The masks are the device's; the proof is ours to make.
            let mut witnesses = Vec::with_capacity(outputs.len());
            for (ack, output) in out_acks.iter().zip(&outputs) {
                let mask = ack.mask.ok_or_else(|| failed("the device kept an output's mask"))?;
                let mask = Scalar::read(&mut &mask[..]).map_err(|_| failed("the device's mask isn't a scalar"))?;
                witnesses.push(Commitment { mask, amount: output.amount });
            }
            if witnesses.iter().map(|w| w.commit().compress()).ne(commitments.iter().copied()) {
                return Err(failed("the device's commitments don't match its masks"));
            }
            let proof = Bulletproof::prove_plus(rng, witnesses).map_err(|e| failed(format!("couldn't make the range proof: {e:?}")))?;
            let serialized = proof.serialize();
            session.expect(m::MONERO_TRANSACTION_SET_OUTPUT_REQUEST, &m::set_output(output, &hmacs[i], Some(&serialized)), m::MONERO_TRANSACTION_SET_OUTPUT_ACK)?;
            bulletproof = Some(proof);
        } else if last {
            let proof = out_acks[i].range_proof.as_ref().ok_or_else(|| failed("the device gave no range proof"))?;
            let proof = Bulletproof::read_plus(&mut proof.as_slice()).map_err(|_| failed("the device's range proof can't be read"))?;
            if !proof.verify(rng, &commitments) {
                return Err(failed("the device's range proof doesn't check out"));
            }
            bulletproof = Some(proof);
        }
        progress(session);
    }
    let bulletproof = bulletproof.ok_or_else(|| failed("no range proof"))?;

    // 5. The transaction, as both sides see it.
    let ack = session.expect(m::MONERO_TRANSACTION_ALL_OUT_SET_REQUEST, &[], m::MONERO_TRANSACTION_ALL_OUT_SET_ACK)?;
    let all = m::all_out_set_ack(&ack).map_err(|e| failed(e.to_string()))?;
    if all.fee != plan.fee || all.rct_type != 6 {
        return Err(failed("the device signed a different fee or transaction type"));
    }
    let inputs = order.iter().map(|&o| Input::read(&mut acks[o].vini.as_slice()).map_err(|_| failed("the device's input can't be read"))).collect::<Result<Vec<_>>>()?;
    let tx_outputs = out_acks.iter().map(|a| Output::read(true, &mut a.tx_out.as_slice()).map_err(|_| failed("the device's output can't be read"))).collect::<Result<Vec<_>>>()?;
    let prefix = TransactionPrefix { additional_timelock: Timelock::None, inputs, outputs: tx_outputs, extra: all.extra.clone() };
    let mut serialized = Transaction::<monero_wallet::transaction::NotPruned>::V2 { prefix: prefix.clone(), proofs: None }.serialize();
    serialized.pop();
    if crypto::keccak(&serialized) != all.tx_prefix_hash {
        return Err(failed("the device signed a different transaction than Fob built"));
    }
    let mut pseudo_outs = order.iter().map(|&o| compressed(&acks[o].pseudo_out)).collect::<Result<Vec<_>>>()?;
    let base = RctBase {
        fee: plan.fee,
        pseudo_outs: vec![],
        encrypted_amounts: out_acks.iter().map(|a| EncryptedAmount::Compact { amount: a.ecdh_info[..].try_into().expect("checked 8") }).collect(),
        commitments: out_acks.iter().map(|a| compressed(&a.out_pk[32..])).collect::<Result<_>>()?,
    };
    let make = |clsags: Vec<Clsag>, pseudo_outs: Vec<CompressedPoint>| Transaction::V2 {
        prefix: prefix.clone(),
        proofs: Some(RctProofs { base: base.clone(), prunable: RctPrunable::Clsag { clsags, pseudo_outs, bulletproof: bulletproof.clone() } }),
    };
    let unsigned = make(vec![], pseudo_outs.clone());
    let message = unsigned.signature_hash().ok_or_else(|| failed("no message to sign"))?;
    if message != all.full_message_hash {
        return Err(failed("the device signs a different message than Fob's transaction gives"));
    }
    progress(session);

    // 6. Signatures, sealed until the end.
    let mut sealed = Vec::with_capacity(order.len());
    for (position, &original) in order.iter().enumerate() {
        let ack = session.expect(m::MONERO_TRANSACTION_SIGN_INPUT_REQUEST, &m::sign_input(&sources[original], &acks[original], original as u32), m::MONERO_TRANSACTION_SIGN_INPUT_ACK)?;
        let (signature, pseudo_out) = m::sign_input_ack(&ack).map_err(|e| failed(e.to_string()))?;
        if let Some(pseudo_out) = pseudo_out {
            pseudo_outs[position] = CompressedPoint::from(pseudo_out);
        }
        sealed.push(signature);
        progress(session);
    }
    let ack = session.expect(m::MONERO_TRANSACTION_FINAL_REQUEST, &[], m::MONERO_TRANSACTION_FINAL_ACK)?;
    let opening = m::final_ack(&ack).map_err(|e| failed(e.to_string()))?;
    let mut clsags = Vec::with_capacity(sealed.len());
    for (i, signature) in sealed.iter().enumerate() {
        let key = crypto::sealing_key(&opening, i, false);
        let nonce = crypto::sealing_key(&opening, i, true);
        let plain = crypto::open(&key, &nonce[..12], signature).map_err(failed)?;
        let mut reader = plain.as_slice();
        let count = read_varint(&mut reader)?;
        if count != ring_len as u64 {
            return Err(failed("the device's signature is for a ring of another size"));
        }
        clsags.push(Clsag::read(ring_len, &mut reader).map_err(|_| failed("the device's signature can't be read"))?);
    }
    let tx = make(clsags, pseudo_outs);
    check(&tx, &sources, &order, &message)?;
    check_outputs(&tx, plan, &outputs, view_key)?;
    progress(session);
    Ok(tx)
}

/// What the device made of the outputs, checked with the view key: the
/// change comes back to this wallet (its one-time key, view tag, commitment
/// and encrypted amount all follow from the transaction key in the extra),
/// the inputs' pseudo outputs balance the outputs and the fee, and each
/// input's offsets are its ring's. A device or cable that lied about any of
/// these would give a transaction that burns the change or that the node
/// refuses; this says so before it leaves.
fn check_outputs(tx: &Transaction, plan: &Plan, outputs: &[Destination], view_key: &[u8; 32]) -> Result<()> {
    use curve25519_dalek::{EdwardsPoint, Scalar as DalekScalar, traits::Identity};
    use monero_wallet::extra::Extra;
    let Transaction::V2 { prefix, proofs: Some(RctProofs { base, prunable: RctPrunable::Clsag { pseudo_outs, .. } }) } = tx else {
        return Err(failed("not a signed transaction"));
    };
    let decompress = |p: &CompressedPoint| p.decompress().map(|p| -> EdwardsPoint { p.into() }).ok_or_else(|| failed("the device gave a key that isn't a point"));
    // Inputs and outputs balance: Σ pseudo outs = Σ commitments + fee·H.
    let mut ins = EdwardsPoint::identity();
    for p in pseudo_outs {
        ins += decompress(p)?;
    }
    let mut outs: EdwardsPoint = Commitment { mask: Scalar::from(DalekScalar::ZERO), amount: plan.fee }.commit().into();
    for c in &base.commitments {
        outs += decompress(c)?;
    }
    if ins != outs {
        return Err(failed("the device's commitments don't balance the fee"));
    }
    // Each input's offsets are its ring's.
    for (input, source) in prefix.inputs.iter().zip(inputs_in_order(prefix, plan)) {
        let Input::ToKey { key_offsets, amount, .. } = input else { return Err(failed("an input isn't a RingCT one")) };
        let mut positions: Vec<u64> = source.ring.iter().map(|r| r.index).collect();
        positions.sort_unstable();
        let mut relative = positions.clone();
        for i in (1..relative.len()).rev() {
            relative[i] -= relative[i - 1];
        }
        if *key_offsets != relative || amount.is_some() {
            return Err(failed("the device's input doesn't name its ring"));
        }
    }
    // The change comes back here.
    if plan.change == 0 {
        return Ok(());
    }
    let change = destination(&plan.change_address, plan.change);
    let i = outputs.iter().position(|o| o.amount == plan.change && o.same_address(&change)).ok_or_else(|| failed("the change is missing"))?;
    let (keys, _) = Extra::read(&mut prefix.extra.as_slice()).ok().and_then(|e| e.keys()).ok_or_else(|| failed("the device's extra has no transaction key"))?;
    let tx_pub: EdwardsPoint = keys.first().copied().ok_or_else(|| failed("the device's extra has no transaction key"))?.into();
    let view = DalekScalar::from_canonical_bytes(*view_key).into_option().ok_or_else(|| failed("the view key isn't canonical"))?;
    let derivation = (tx_pub * view).mul_by_cofactor().compress().to_bytes();
    let mut data = derivation.to_vec();
    data.extend_from_slice(&super::proto::varint(i as u64));
    let shared = Scalar::hash(&data);
    let shared_bytes = <[u8; 32]>::from(shared);
    let spend: EdwardsPoint = plan.change_address.spend().into();
    let shared_scalar: DalekScalar = shared.into();
    let one_time = (&shared_scalar * curve25519_dalek::constants::ED25519_BASEPOINT_TABLE + spend).compress().to_bytes();
    let output = prefix.outputs.get(i).ok_or_else(|| failed("the change is missing"))?;
    if output.key.to_bytes() != one_time {
        return Err(failed("the change doesn't come back to this wallet"));
    }
    let view_tag = crypto::keccak(&[b"view_tag".as_slice(), &data].concat())[0];
    if output.view_tag != Some(view_tag) {
        return Err(failed("the change's view tag is wrong: this wallet wouldn't see it"));
    }
    let mask = Scalar::hash([b"commitment_mask".as_slice(), &shared_bytes].concat());
    if (Commitment { mask, amount: plan.change }).commit().compress() != base.commitments[i] {
        return Err(failed("the change's commitment isn't for the change"));
    }
    let amount_mask = crypto::keccak(&[b"amount".as_slice(), &shared_bytes].concat());
    let EncryptedAmount::Compact { amount } = &base.encrypted_amounts[i] else { return Err(failed("the change's amount isn't compact")) };
    if u64::from_le_bytes(*amount) ^ u64::from_le_bytes(amount_mask[..8].try_into().expect("8")) != plan.change {
        return Err(failed("the change's encrypted amount isn't the change"));
    }
    Ok(())
}

/// The plan's inputs in the transaction's order: by key image, as
/// [`sign`] sorted them, which the transaction's inputs now carry.
fn inputs_in_order(prefix: &TransactionPrefix, plan: &Plan) -> Vec<Source> {
    // Each input's ring has one real member; the device's key image belongs
    // to that output. Match by the ring's positions, which are each input's own.
    let sources: Vec<Source> = plan.inputs.iter().map(source).collect();
    prefix
        .inputs
        .iter()
        .filter_map(|input| {
            let Input::ToKey { key_offsets, .. } = input else { return None };
            let mut positions = Vec::with_capacity(key_offsets.len());
            let mut at = 0u64;
            for o in key_offsets {
                at = at.wrapping_add(*o);
                positions.push(at);
            }
            sources.iter().find(|s| {
                let mut theirs: Vec<u64> = s.ring.iter().map(|r| r.index).collect();
                theirs.sort_unstable();
                theirs == positions
            })
        })
        .cloned()
        .collect()
}

fn read_varint(data: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (byte, rest) = data.split_first().ok_or_else(|| failed("the device's signature is cut short"))?;
        *data = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(failed("the device's signature is malformed"))
}

/// Every CLSAG verified against its ring, key image and pseudo output.
fn check(tx: &Transaction, sources: &[Source], order: &[usize], message: &[u8; 32]) -> Result<()> {
    let Transaction::V2 { prefix, proofs: Some(RctProofs { prunable: RctPrunable::Clsag { clsags, pseudo_outs, .. }, .. }) } = tx else {
        return Err(failed("not a signed transaction"));
    };
    for (position, &original) in order.iter().enumerate() {
        let Some(Input::ToKey { key_image, .. }) = prefix.inputs.get(position) else { return Err(failed("an input is missing")) };
        let ring = sources[original].ring.iter().map(|r| [CompressedPoint::from(r.key), CompressedPoint::from(r.commitment)]).collect();
        clsags
            .get(position)
            .ok_or_else(|| failed("a signature is missing"))?
            .verify(ring, key_image, &pseudo_outs[position], message)
            .map_err(|e| failed(format!("a signature from the device doesn't verify: {e:?}")))?;
    }
    Ok(())
}

impl Session<'_> {
    pub(super) fn prompts_progress(&self, done: u32, total: u32) {
        self.prompts().progress(done, total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_read_as_monero_writes_them() {
        let mut data: &[u8] = &[0x10, 0xff];
        assert_eq!(read_varint(&mut data).unwrap(), 16);
        assert_eq!(data, &[0xff]);
        let mut data: &[u8] = &[0xac, 0x02];
        assert_eq!(read_varint(&mut data).unwrap(), 300);
        let mut data: &[u8] = &[0x80];
        assert!(read_varint(&mut data).is_err());
    }

    #[test]
    fn key_images_are_the_input_s_last_bytes() {
        let mut vini = vec![2u8, 0, 1, 5];
        vini.extend_from_slice(&[7; 32]);
        assert_eq!(key_image_of(&vini).unwrap(), [7; 32]);
        assert!(key_image_of(&[1, 2]).is_err());
    }
}
