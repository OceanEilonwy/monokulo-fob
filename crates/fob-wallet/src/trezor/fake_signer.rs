//! The signing half of the software Trezor tests use
//! ([`session::fake::Fake`](super::session::fake::Fake)): Monero's
//! cold-signing protocol answered as trezor-core answers it, with the
//! wallet's spend key, so that [`sign`](super::sign::sign) runs end to end
//! and its checks (prefix and message hashes, range proof, change, balance,
//! every CLSAG) hold against a real signature. Standard addresses only: no
//! additional transaction keys, no payment ids. The range proof is the
//! device's, made with the last output, as a device does for two outputs.

use std::sync::Mutex;

use curve25519_dalek::{EdwardsPoint, Scalar as DScalar, constants::ED25519_BASEPOINT_TABLE};
use monero_wallet::{
    ed25519::{Commitment, CompressedPoint, Point, Scalar},
    extra::ExtraField,
    ringct::{
        EncryptedAmount, RctBase, RctProofs, RctPrunable,
        bulletproofs::Bulletproof,
        clsag::{Clsag, ClsagContext, Decoys},
    },
    transaction::{Input, NotPruned, Output, Timelock, Transaction, TransactionPrefix},
};
use rand_core::OsRng;
use zeroize::Zeroizing;

use super::crypto;
use super::messages as m;
use super::proto::{Reader, Writer, varint};

/// A device's spend and view keys, and the payment it's in the middle of.
pub struct Signer {
    spend: DScalar,
    view: DScalar,
    signing: Mutex<Option<Signing>>,
}

/// An output as the device made it.
struct Made {
    tx_out: Vec<u8>,
    commitment: Commitment,
    encrypted: [u8; 8],
}

/// An input as the device took it.
struct Taken {
    secret: DScalar,
    vini: Vec<u8>,
    image: [u8; 32],
    amount: u64,
    mask: Scalar,
}

struct Signing {
    r: DScalar,
    fee: u64,
    destinations: Vec<(u64, [u8; 32], [u8; 32])>,
    inputs: Vec<Taken>,
    made: Vec<Made>,
    bulletproof: Option<Bulletproof>,
    message: [u8; 32],
    /// Pseudo output masks handed out so far, in the transaction's order.
    masks: Vec<DScalar>,
    opening: [u8; 32],
}

fn point(bytes: &[u8]) -> EdwardsPoint {
    curve25519_dalek::edwards::CompressedEdwardsY(bytes.try_into().expect("32 bytes")).decompress().expect("a point")
}

fn random() -> DScalar {
    Scalar::random(&mut OsRng).into()
}

/// `Hs(8·secret·public ‖ index)`, and the derivation it hashes.
fn shared(secret: &DScalar, public: &EdwardsPoint, index: u64) -> (Scalar, Vec<u8>) {
    let mut data = (public * secret).mul_by_cofactor().compress().to_bytes().to_vec();
    data.extend_from_slice(&varint(index));
    (Scalar::hash(&data), data)
}

/// A ring member: its index, and its key and commitment where given.
fn ring(source: &Reader) -> (Vec<u64>, Vec<Option<[Point; 2]>>) {
    let members = source.all_messages(1).expect("members");
    let indices = members.iter().map(|r| r.uint(1).expect("an index")).collect();
    let keys = members
        .iter()
        .map(|r| {
            let key = r.message(2).ok().flatten()?;
            let decompress = |b: &[u8]| CompressedPoint::from(<[u8; 32]>::try_from(b).ok()?).decompress();
            Some([decompress(key.bytes(1)?)?, decompress(key.bytes(2)?)?])
        })
        .collect();
    (indices, keys)
}

fn relative(indices: &[u64]) -> Vec<u64> {
    let mut relative = indices.to_vec();
    for i in (1..relative.len()).rev() {
        relative[i] -= relative[i - 1];
    }
    relative
}

impl Signer {
    pub fn new(spend: DScalar, view: DScalar) -> Signer {
        Signer { spend, view, signing: Mutex::new(None) }
    }

    /// The answer to one signing request, or none for a request that isn't one.
    pub fn answer(&self, msg_type: u16, data: &[u8]) -> Option<(u16, Vec<u8>)> {
        let r = Reader::parse(data).expect("a request");
        let mut signing = self.signing.lock().unwrap();
        let mut w = Writer::new();
        let answer = match msg_type {
            m::MONERO_TRANSACTION_INIT_REQUEST => {
                let tsx = r.message(4).unwrap().expect("the transaction");
                let destinations: Vec<(u64, [u8; 32], [u8; 32])> = tsx
                    .all_messages(4)
                    .unwrap()
                    .iter()
                    .map(|d| {
                        assert!(!d.bool(3).unwrap_or(false), "the fake pays standard addresses only");
                        let address = d.message(2).unwrap().unwrap();
                        (d.uint(1).unwrap_or(0), address.bytes(1).unwrap().try_into().unwrap(), address.bytes(2).unwrap().try_into().unwrap())
                    })
                    .collect();
                for i in 0..destinations.len() {
                    w.bytes(1, &[i as u8; 32]);
                }
                let mut opening = [0; 32];
                rand_core::RngCore::fill_bytes(&mut OsRng, &mut opening);
                *signing = Some(Signing {
                    r: random(),
                    fee: tsx.uint(8).unwrap_or(0),
                    destinations,
                    inputs: vec![],
                    made: vec![],
                    bulletproof: None,
                    message: [0; 32],
                    masks: vec![],
                    opening,
                });
                m::MONERO_TRANSACTION_INIT_ACK
            }
            m::MONERO_TRANSACTION_SET_INPUT_REQUEST => {
                let signing = signing.as_mut().expect("initialised");
                let source = r.message(1).unwrap().unwrap();
                let (indices, keys) = ring(&source);
                let real = source.uint(2).unwrap() as usize;
                let out_key = keys[real].expect("the real member's key")[0];
                let index_in_tx = source.uint(5).unwrap_or(0);
                // The output's secret key from whichever transaction key made it.
                let minor = source.uint(10).unwrap_or(0) as u32;
                let sub = if minor == 0 {
                    DScalar::ZERO
                } else {
                    let mut data = b"SubAddr\0".to_vec();
                    data.extend_from_slice(self.view.as_bytes());
                    data.extend_from_slice(&0u32.to_le_bytes());
                    data.extend_from_slice(&minor.to_le_bytes());
                    Scalar::hash(&data).into()
                };
                let secret = [source.bytes(3), source.bytes(4)]
                    .into_iter()
                    .flatten()
                    .map(|key| shared(&self.view, &point(key), index_in_tx).0.into() + self.spend + sub)
                    .find(|x: &DScalar| (x * ED25519_BASEPOINT_TABLE) == out_key.into())
                    .expect("the device owns the output");
                let image = (Point::biased_hash(out_key.compress().to_bytes()).into() * secret).compress().to_bytes();
                let input = Input::ToKey { amount: None, key_offsets: relative(&indices), key_image: CompressedPoint::from(image) };
                let mut vini = vec![];
                input.write(&mut vini).unwrap();
                let mask = Scalar::read(&mut source.bytes(8).unwrap()).unwrap();
                signing.inputs.push(Taken { secret, vini: vini.clone(), image, amount: source.uint(6).unwrap_or(0), mask });
                let placeholder = (&DScalar::ONE * ED25519_BASEPOINT_TABLE).compress().to_bytes();
                w.bytes(1, &vini).bytes(2, &[0; 32]).bytes(3, &placeholder).bytes(4, &[0; 32]).bytes(5, &[0; 32]).bytes(6, &[0; 32]);
                m::MONERO_TRANSACTION_SET_INPUT_ACK
            }
            m::MONERO_TRANSACTION_INPUT_VINI_REQUEST => m::MONERO_TRANSACTION_INPUT_VINI_ACK,
            m::MONERO_TRANSACTION_ALL_INPUTS_SET_REQUEST => m::MONERO_TRANSACTION_ALL_INPUTS_SET_ACK,
            m::MONERO_TRANSACTION_SET_OUTPUT_REQUEST => {
                let signing = signing.as_mut().expect("initialised");
                let i = signing.made.len();
                let (amount, spend, view) = signing.destinations[i];
                let (s, derivation) = shared(&signing.r, &point(&view), i as u64);
                let s_d: DScalar = s.into();
                let key = (&s_d * ED25519_BASEPOINT_TABLE + point(&spend)).compress().to_bytes();
                let view_tag = crypto::keccak(&[b"view_tag".as_slice(), &derivation].concat())[0];
                let s_bytes = <[u8; 32]>::from(s);
                let commitment = Commitment { mask: Scalar::hash([b"commitment_mask".as_slice(), &s_bytes].concat()), amount };
                let pad = crypto::keccak(&[b"amount".as_slice(), &s_bytes].concat());
                let encrypted = (amount ^ u64::from_le_bytes(pad[..8].try_into().unwrap())).to_le_bytes();
                let mut tx_out = vec![];
                Output { amount: None, key: CompressedPoint::from(key), view_tag: Some(view_tag) }.write(&mut tx_out).unwrap();
                let mut out_pk = key.to_vec();
                out_pk.extend_from_slice(&commitment.commit().compress().to_bytes());
                w.bytes(1, &tx_out).bytes(2, &[0; 32]);
                signing.made.push(Made { tx_out, commitment: commitment.clone(), encrypted });
                if signing.made.len() == signing.destinations.len() {
                    let proof = Bulletproof::prove_plus(&mut OsRng, signing.made.iter().map(|o| o.commitment.clone()).collect()).unwrap();
                    let mut rsig = Writer::new();
                    rsig.bytes(5, &proof.serialize());
                    w.message(3, &rsig);
                    signing.bulletproof = Some(proof);
                }
                w.bytes(4, &out_pk).bytes(5, &encrypted);
                m::MONERO_TRANSACTION_SET_OUTPUT_ACK
            }
            m::MONERO_TRANSACTION_ALL_OUT_SET_REQUEST => {
                let signing = signing.as_mut().expect("initialised");
                // Inputs by key image, largest first, as Monero orders them.
                signing.inputs.sort_by_key(|i| std::cmp::Reverse(i.image));
                let r_pub = (&signing.r * ED25519_BASEPOINT_TABLE).compress().to_bytes();
                let extra = ExtraField::PublicKey(CompressedPoint::from(r_pub)).serialize();
                let prefix = TransactionPrefix {
                    additional_timelock: Timelock::None,
                    inputs: signing.inputs.iter().map(|i| Input::read(&mut i.vini.as_slice()).unwrap()).collect(),
                    outputs: signing.made.iter().map(|o| Output::read(true, &mut o.tx_out.as_slice()).unwrap()).collect(),
                    extra: extra.clone(),
                };
                let mut serialized = Transaction::<NotPruned>::V2 { prefix: prefix.clone(), proofs: None }.serialize();
                serialized.pop();
                let base = RctBase {
                    fee: signing.fee,
                    pseudo_outs: vec![],
                    encrypted_amounts: signing.made.iter().map(|o| EncryptedAmount::Compact { amount: o.encrypted }).collect(),
                    commitments: signing.made.iter().map(|o| o.commitment.commit().compress()).collect(),
                };
                let placeholders = vec![CompressedPoint::from(r_pub); signing.inputs.len()];
                let unsigned = Transaction::<NotPruned>::V2 {
                    prefix,
                    proofs: Some(RctProofs { base, prunable: RctPrunable::Clsag { clsags: vec![], pseudo_outs: placeholders, bulletproof: signing.bulletproof.clone().unwrap() } }),
                };
                signing.message = unsigned.signature_hash().unwrap();
                let mut rv = Writer::new();
                rv.uint(1, signing.fee).uint(3, 6);
                w.bytes(1, &extra).bytes(2, &crypto::keccak(&serialized)).message(4, &rv).bytes(5, &signing.message);
                m::MONERO_TRANSACTION_ALL_OUT_SET_ACK
            }
            m::MONERO_TRANSACTION_SIGN_INPUT_REQUEST => {
                let signing = signing.as_mut().expect("initialised");
                let position = signing.masks.len();
                let source = r.message(1).unwrap().unwrap();
                let (indices, keys) = ring(&source);
                let real = source.uint(2).unwrap() as u8;
                let vini = r.bytes(2).unwrap();
                let input = signing.inputs.iter().find(|i| i.vini == vini).expect("an input the device took");
                assert_eq!(signing.inputs[position].vini, vini, "signed in the transaction's order");
                let members = keys.into_iter().map(|k| k.expect("every member's key")).collect();
                let decoys = Decoys::new(relative(&indices), real, members).unwrap();
                let context = ClsagContext::new(decoys, Commitment { mask: input.mask, amount: input.amount }).unwrap();
                // The last input's mask balances the outputs'; the others are random.
                let mask = if position + 1 == signing.inputs.len() {
                    signing.made.iter().map(|o| o.commitment.mask.into()).sum::<DScalar>() - signing.masks.iter().sum::<DScalar>()
                } else {
                    random()
                };
                signing.masks.push(mask);
                let (clsag, pseudo_out) = Clsag::sign(&mut OsRng, vec![(Zeroizing::new(Scalar::from(input.secret)), context)], Scalar::from(mask), signing.message).unwrap().remove(0);
                let mut plain = varint(indices.len() as u64);
                clsag.write(&mut plain).unwrap();
                let key = crypto::sealing_key(&signing.opening, position, false);
                let nonce = crypto::sealing_key(&signing.opening, position, true);
                w.bytes(1, &crypto::seal(&key, nonce[..12].try_into().unwrap(), &plain)).bytes(2, &pseudo_out.compress().to_bytes());
                m::MONERO_TRANSACTION_SIGN_INPUT_ACK
            }
            m::MONERO_TRANSACTION_FINAL_REQUEST => {
                let signing = signing.take().expect("initialised");
                w.bytes(5, &signing.opening);
                m::MONERO_TRANSACTION_FINAL_ACK
            }
            _ => return None,
        };
        Some((answer, w.finish()))
    }
}
