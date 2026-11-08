//! Sending through a light wallet server (phase 3 of the Light Wallet
//! proposal): the fee rate from `/get_unspent_outs`, each input's ring from
//! the members `/get_random_outs` gives, and the signed transaction handed
//! to `/submit_raw_tx`. The coins spent are the wallet's own records, found
//! and checked by light syncing; the transaction is built and signed here,
//! as the node path builds and signs it.

use super::*;
use crate::light::{LightClient, Problem, lws};
use curve25519_dalek::traits::IsIdentity;
use monero_wallet::{
    ed25519::{CompressedPoint, Point},
    ringct::clsag::Decoys as Ring,
};

/// The most rings `/get_random_outs` answers at once: monero-lws's limit,
/// which monokulo-lws keeps too. A payment with more inputs asks again.
const MAX_RINGS: usize = 150;

/// Where in `/get_unspent_outs`' `fees` (the four priorities, lowest
/// first) the fee Fob pays is: `Normal`, as monero-daemon-rpc picks it from
/// a node's `get_fee_estimate`, which is what the node path pays.
const NORMAL: usize = 1;

/// Where a payment's rings and fee come from.
pub(super) enum Source {
    /// The node: monero-oxide picks decoys from its output distribution, up
    /// to the tip `latest`.
    Node { decoys: Decoys, latest: usize },
    /// A light wallet server, which picks them itself.
    Light(LightClient),
}

/// The fee rate a server's `/get_unspent_outs` gives: its `fees` for the
/// node path's priority, or its `per_byte_fee` where it gives no `fees`
/// (monero-lws before it had them), with its `fee_mask`. Refused when it's
/// more per byte than [`MAX_FEE_PER_WEIGHT`], or its rounding more than
/// [`MAX_FEE_MASK`], the caps the node path holds a node to: a server that
/// lies can't spend the payment's money on its fee. Not a fee rate at all
/// (nothing, or a priority missing) is an answer Fob can't read.
pub(super) fn light_fee_rate(unspent: &lws::UnspentOuts) -> std::result::Result<FeeRate, Problem> {
    let per_byte = match unspent.fees.as_slice() {
        [] => unspent.per_byte_fee.0,
        fees => *fees.get(NORMAL).ok_or(Problem::Unreadable)?,
    };
    let mask = unspent.fee_mask.0;
    if per_byte > MAX_FEE_PER_WEIGHT || mask > MAX_FEE_MASK {
        return Err(Problem::FeeTooHigh);
    }
    FeeRate::new(per_byte, mask).ok_or(Problem::Unreadable)
}

/// One ring member as the server gave it, read.
struct Member {
    index: u64,
    key: Point,
    commitment: Point,
}

/// Reads one member: its key and commitment (the first 32 bytes of `rct`,
/// which is 32 bytes or, for old clients, 96 with the rest zero) must be
/// points.
fn member(given: &lws::RandomOutput) -> std::result::Result<Member, Problem> {
    let point = |bytes: &[u8]| -> Option<Point> { CompressedPoint::from(<[u8; 32]>::try_from(bytes).ok()?).decompress() };
    if given.rct.0.len() != 32 && given.rct.0.len() != 96 {
        return Err(Problem::BadRing);
    }
    let key = point(&given.public_key.0).ok_or(Problem::BadRing)?;
    let commitment = point(&given.rct.0[..32]).ok_or(Problem::BadRing)?;
    Ok(Member { index: given.global_index.0, key, commitment })
}

/// Whether a member could be signed for as a real ring member: no torsion,
/// and not the identity (monero-oxide's own decoy picker skips both).
fn usable(member: &Member) -> bool {
    member.key.into().is_torsion_free() && member.commitment.into().is_torsion_free() && !member.key.into().is_identity()
}

/// `real`'s ring from the members the server gave for it (`count` of them,
/// [`RING_LEN`], one more than the decoys a ring needs), as monero-oxide's
/// `OutputWithDecoys::new` would make it from decoys of its own: the real
/// output and [`RING_LEN`] − 1 members, sorted by global index, offsets
/// relative, the real one's place the signer's.
///
/// The answer must be RingCT's (amount 0), exactly [`RING_LEN`] members,
/// every key and commitment a point, no global index twice; and a member at
/// the real output's global index must be the real output, key and
/// commitment (the server's own view of the coin, as monero-oxide checks
/// the node's), else [`Problem::BadRing`] and nothing is sent. The member
/// that is the real output, and any that can't be signed for (torsion, the
/// identity), are dropped; of the rest, one at random goes, as
/// [`RING_LEN`] − 1 are needed. Too few left is a bad ring too.
///
/// `OutputWithDecoys` has no constructor for a ring picked elsewhere; its
/// `read` is public, and its bytes are the coin's (key, key offset,
/// commitment) and then the ring's `Decoys`, which `Decoys::new` checks.
pub(super) fn ring(real: &WalletOutput, given: &lws::RandomOutputs, rng: &mut impl RngCore) -> std::result::Result<OutputWithDecoys, Problem> {
    let needed = usize::from(RING_LEN) - 1;
    if given.amount.0 != 0 || given.outputs.len() != usize::from(RING_LEN) {
        return Err(Problem::BadRing);
    }
    let read = given.outputs.iter().map(member).collect::<std::result::Result<Vec<_>, _>>()?;
    let mut indices = HashSet::new();
    if !read.iter().all(|m| indices.insert(m.index)) {
        return Err(Problem::BadRing);
    }
    let own = real.index_on_blockchain();
    let (own_key, own_commitment) = (real.key(), real.commitment().commit());
    let mut decoys = Vec::with_capacity(read.len());
    for m in read {
        if m.index == own {
            if m.key != own_key || m.commitment != own_commitment {
                return Err(Problem::BadRing);
            }
            continue;
        }
        if usable(&m) {
            decoys.push(m);
        }
    }
    if decoys.len() < needed {
        return Err(Problem::BadRing);
    }
    while decoys.len() > needed {
        let at = (rng.next_u64() % decoys.len() as u64) as usize;
        decoys.swap_remove(at);
    }
    let mut members: Vec<(u64, [Point; 2])> = decoys.into_iter().map(|m| (m.index, [m.key, m.commitment])).collect();
    members.push((own, [own_key, own_commitment]));
    members.sort_by_key(|m| m.0);
    let offsets = members.iter().scan(0, |previous, m| Some(m.0 - std::mem::replace(previous, m.0))).collect();
    let signer = u8::try_from(members.partition_point(|m| m.0 < own)).map_err(|_| Problem::BadRing)?;
    let decoys = Ring::new(offsets, signer, members.into_iter().map(|m| m.1).collect()).ok_or(Problem::BadRing)?;
    let mut bytes = Vec::with_capacity(32 + 32 + 40 + 16 * 64 + 20);
    bytes.extend_from_slice(&own_key.compress().to_bytes());
    real.key_offset().write(&mut bytes).expect("writing to a Vec");
    real.commitment().write(&mut bytes).expect("writing to a Vec");
    decoys.write(&mut bytes).expect("writing to a Vec");
    OutputWithDecoys::read(&mut bytes.as_slice()).map_err(|_| Problem::BadRing)
}

/// Each of `reals`' rings from one `/get_random_outs` answer, which must
/// have a ring for each, in order.
pub(super) fn rings(reals: &[WalletOutput], answer: &lws::RandomOuts, rng: &mut impl RngCore) -> std::result::Result<Vec<OutputWithDecoys>, Problem> {
    if answer.amount_outs.len() != reals.len() {
        return Err(Problem::BadRing);
    }
    reals.iter().zip(&answer.amount_outs).map(|(real, given)| ring(real, given, rng)).collect()
}

/// What a server's answer to `/submit_raw_tx` means for the payment.
/// "OK" is published, unless it says it didn't pass it on
/// (`not_relayed`): its node holds it but the network may never see it,
/// so it's recorded and offered again, like a payment the node gave no
/// answer for. Any other status is the node turning it down, in the
/// server's words ([`refusal`]). No answer, or one that can't be read, may
/// still have been taken, unless the request never reached the server.
pub(super) fn submitted(answer: std::result::Result<lws::SubmitResponse, Problem>) -> std::result::Result<(), Unpublished> {
    match answer {
        Ok(answer) if answer.status == "OK" && answer.not_relayed != Some(true) => Ok(()),
        Ok(answer) if answer.status == "OK" => Err(Unpublished::Unsure),
        Ok(answer) => Err(Unpublished::Refused(FobError::Light(Problem::TxRefused(refusal(answer.reason.as_deref().unwrap_or("")))))),
        Err(Problem::Unreachable(text)) if transport::never_reached(&text) => Err(Unpublished::Refused(FobError::Light(Problem::Unreachable(text)))),
        Err(Problem::Unreachable(_) | Problem::Unreadable | Problem::Unavailable | Problem::Status(500..)) => Err(Unpublished::Unsure),
        Err(problem) => Err(Unpublished::Refused(FobError::Light(problem))),
    }
}

/// A server's reason for a refusal, as it's said: one line of printable
/// characters, at most 120, so a server can't fill the screen or put
/// control characters on it. It's the server's own words about the
/// transaction; nothing of the wallet's is in it.
pub(super) fn refusal(reason: &str) -> String {
    let line: String = reason.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(120) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line,
    }
}

impl Wallet {
    /// Why Send can't pay from this wallet as it's set up, or none: a
    /// Trezor wallet syncing through a light wallet server sends through a
    /// node, which its key images come from.
    pub async fn send_notice(&self) -> Option<String> {
        (self.inner.keys.device().is_some() && self.light_mode().await).then(|| fob_words::light::TREZOR_SENDS_THROUGH_NODE.to_owned())
    }

    /// Asks the server the network fee (`/get_unspent_outs`, whose outputs
    /// aren't used: the coins spent are the wallet's own records), capped
    /// as [`light_fee_rate`] says, and keeps it for Split up's estimate.
    pub(super) async fn light_fee(&self, client: &LightClient) -> Result<FeeRate> {
        let request = lws::UnspentOutsRequest { login: self.light_login(), amount: lws::Amount(0), mixin: 0, use_dust: true, dust_threshold: None };
        let unspent = client.get_unspent_outs(&request).await.map_err(FobError::Light)?;
        let fee_rate = light_fee_rate(&unspent).map_err(FobError::Light)?;
        *self.inner.fee_rate.lock().expect("fee lock") = Some((fee_rate.per_weight(), unspent.fee_mask.0));
        Ok(fee_rate)
    }

    /// The fee rate a payment pays, and where its rings come from: the
    /// node, its decoy distribution brought up to the tip; or the light
    /// wallet server syncing is through.
    pub(super) async fn send_source(&self) -> Result<(FeeRate, Source)> {
        if self.light_mode().await {
            let client = self.light_client().await?;
            let fee_rate = self.light_fee(&client).await?;
            return Ok((fee_rate, Source::Light(client)));
        }
        let (_, rpc, decoys, _) = self.connection().await?;
        let (fee_rate, latest) = self.send_ready(&rpc, &decoys).await?;
        Ok((fee_rate, Source::Node { decoys, latest }))
    }

    /// Each of `outputs`' rings. From the node, every input's at once (each
    /// is a round trip); from a server, one `/get_random_outs` for all of
    /// them (in requests of [`MAX_RINGS`]), [`RING_LEN`] members each.
    pub(super) async fn rings(&self, source: &Source, outputs: Vec<WalletOutput>) -> Result<Vec<OutputWithDecoys>> {
        match source {
            Source::Node { decoys, latest } => {
                let mut rings = tokio::task::JoinSet::new();
                for (i, output) in outputs.into_iter().enumerate() {
                    let (decoys, latest) = (decoys.clone(), *latest);
                    rings.spawn(async move { (i, OutputWithDecoys::new(&mut OsRng, &decoys, RING_LEN, latest, output).await) });
                }
                let mut with_decoys: Vec<Option<OutputWithDecoys>> = vec![None; rings.len()];
                while let Some(done) = rings.join_next().await {
                    let (i, ring) = done.map_err(|e| FobError::Node(format!("{e}")))?;
                    with_decoys[i] = Some(ring.map_err(|e| FobError::Node(format!("{e:?}")))?);
                }
                Ok(with_decoys.into_iter().flatten().collect())
            }
            Source::Light(client) => {
                let mut with_decoys = Vec::with_capacity(outputs.len());
                for some in outputs.chunks(MAX_RINGS) {
                    let answer = client.get_random_outs(u32::from(RING_LEN), vec![0; some.len()]).await.map_err(FobError::Light)?;
                    with_decoys.extend(rings(some, &answer, &mut OsRng).map_err(FobError::Light)?);
                }
                Ok(with_decoys)
            }
        }
    }

    /// Hands a signed payment to the light wallet server to broadcast.
    pub(super) async fn submit_through_server(&self, tx: Vec<u8>) -> std::result::Result<(), Unpublished> {
        let client = self.light_client().await.map_err(Unpublished::Refused)?;
        submitted(client.submit_raw_tx(tx).await)
    }

    /// Offers the server again each payment it gave no answer for (or held
    /// without passing on), as the node path offers its node, until it
    /// takes it or its node turns it down; never once it's mined, released
    /// or marked not confirmed.
    pub(super) async fn publish_unanswered_through(&self, client: &LightClient) {
        let waiting: Vec<(String, String)> = {
            let state = self.inner.state.lock().await;
            state.sent.iter().filter(|s| s.height.is_none() && !s.released && !s.not_confirmed).filter_map(|s| Some((s.tx.clone(), s.signed.clone()?))).collect()
        };
        let mut answered = vec![];
        for (tx, signed) in waiting {
            let Ok(bytes) = hex::decode(&signed) else {
                answered.push(tx);
                continue;
            };
            match submitted(client.submit_raw_tx(bytes).await) {
                Ok(()) => answered.push(tx),
                // Turned down: mined already, its coins spent, or never
                // valid. The server's list, or the day-old check, says which.
                Err(Unpublished::Refused(FobError::Light(Problem::TxRefused(reason)))) if super::send::turned_down(&reason) => answered.push(tx),
                Err(_) => {}
            }
        }
        if answered.is_empty() {
            return;
        }
        let mut state = self.inner.state.lock().await;
        for sent in state.sent.iter_mut().filter(|s| answered.contains(&s.tx)) {
            sent.signed = None;
        }
        let _ = self.save_state(&state);
    }
}
