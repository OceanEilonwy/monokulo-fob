//! The messages Fob exchanges with a Trezor, by number and field, after
//! `messages.proto`, `messages-common.proto`, `messages-management.proto`
//! and `messages-monero.proto` in trezor-firmware.

use zeroize::Zeroizing;

use super::proto::{Malformed, Reader, Writer};
use crate::network::Network;

// Message types.
pub const INITIALIZE: u16 = 0;
pub const SUCCESS: u16 = 2;
pub const FAILURE: u16 = 3;
pub const FEATURES: u16 = 17;
pub const PIN_MATRIX_REQUEST: u16 = 18;
pub const PIN_MATRIX_ACK: u16 = 19;
pub const CANCEL: u16 = 20;
pub const BUTTON_REQUEST: u16 = 26;
pub const BUTTON_ACK: u16 = 27;
pub const PASSPHRASE_REQUEST: u16 = 41;
pub const PASSPHRASE_ACK: u16 = 42;
pub const MONERO_TRANSACTION_INIT_REQUEST: u16 = 501;
pub const MONERO_TRANSACTION_INIT_ACK: u16 = 502;
pub const MONERO_TRANSACTION_SET_INPUT_REQUEST: u16 = 503;
pub const MONERO_TRANSACTION_SET_INPUT_ACK: u16 = 504;
pub const MONERO_TRANSACTION_INPUT_VINI_REQUEST: u16 = 507;
pub const MONERO_TRANSACTION_INPUT_VINI_ACK: u16 = 508;
pub const MONERO_TRANSACTION_ALL_INPUTS_SET_REQUEST: u16 = 509;
pub const MONERO_TRANSACTION_ALL_INPUTS_SET_ACK: u16 = 510;
pub const MONERO_TRANSACTION_SET_OUTPUT_REQUEST: u16 = 511;
pub const MONERO_TRANSACTION_SET_OUTPUT_ACK: u16 = 512;
pub const MONERO_TRANSACTION_ALL_OUT_SET_REQUEST: u16 = 513;
pub const MONERO_TRANSACTION_ALL_OUT_SET_ACK: u16 = 514;
pub const MONERO_TRANSACTION_SIGN_INPUT_REQUEST: u16 = 515;
pub const MONERO_TRANSACTION_SIGN_INPUT_ACK: u16 = 516;
pub const MONERO_TRANSACTION_FINAL_REQUEST: u16 = 517;
pub const MONERO_TRANSACTION_FINAL_ACK: u16 = 518;
pub const MONERO_KEY_IMAGE_EXPORT_INIT_REQUEST: u16 = 530;
pub const MONERO_KEY_IMAGE_EXPORT_INIT_ACK: u16 = 531;
pub const MONERO_KEY_IMAGE_SYNC_STEP_REQUEST: u16 = 532;
pub const MONERO_KEY_IMAGE_SYNC_STEP_ACK: u16 = 533;
pub const MONERO_KEY_IMAGE_SYNC_FINAL_REQUEST: u16 = 534;
pub const MONERO_KEY_IMAGE_SYNC_FINAL_ACK: u16 = 535;
pub const MONERO_GET_ADDRESS: u16 = 540;
pub const MONERO_ADDRESS: u16 = 541;
pub const MONERO_GET_WATCH_KEY: u16 = 542;
pub const MONERO_WATCH_KEY: u16 = 543;

/// `Failure.code` for the person saying no, on the device or to a PIN.
pub const FAILURE_ACTION_CANCELLED: u64 = 4;
pub const FAILURE_PIN_CANCELLED: u64 = 6;
pub const FAILURE_PIN_INVALID: u64 = 7;
pub const FAILURE_UNEXPECTED_MESSAGE: u64 = 1;
pub const FAILURE_DATA_ERROR: u64 = 3;
pub const FAILURE_PROCESS_ERROR: u64 = 9;
pub const FAILURE_NOT_ENOUGH_FUNDS: u64 = 10;
pub const FAILURE_NOT_INITIALIZED: u64 = 11;
pub const FAILURE_BUSY: u64 = 15;

/// `Features.capabilities` values.
pub const CAPABILITY_MONERO: u64 = 9;
pub const CAPABILITY_PASSPHRASE_ENTRY: u64 = 17;

/// Monero's BIP-32 path on a Trezor: `m/44'/128'/0'`. The wallet's keys
/// follow from the node there, so one Trezor is always one Monero wallet.
pub const PATH: [u32; 3] = [0x8000_002c, 0x8000_0080, 0x8000_0000];

/// `MoneroNetworkType`.
pub fn network_type(network: Network) -> u64 {
    match network {
        Network::Mainnet => 0,
        Network::Stagenet => 2,
    }
}

fn path(w: &mut Writer, field: u32) {
    for p in PATH {
        w.uint(field, u64::from(p));
    }
}

pub fn initialize(session_id: Option<&[u8]>) -> Vec<u8> {
    let mut w = Writer::new();
    if let Some(id) = session_id {
        w.bytes(1, id);
    }
    w.finish()
}

/// What `Features` says of the device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub vendor: String,
    pub version: (u64, u64, u64),
    pub label: Option<String>,
    pub model: Option<String>,
    pub initialized: bool,
    pub pin_protection: bool,
    pub passphrase_protection: bool,
    pub unlocked: bool,
    pub capabilities: Vec<u64>,
    pub session_id: Option<Vec<u8>>,
}

impl Features {
    pub fn parse(data: &[u8]) -> Result<Features, Malformed> {
        let r = Reader::parse(data)?;
        Ok(Features {
            vendor: r.string(1).unwrap_or_default(),
            version: (r.uint(2).unwrap_or(0), r.uint(3).unwrap_or(0), r.uint(4).unwrap_or(0)),
            label: r.string(10).filter(|s| !s.is_empty()),
            model: r.string(21).filter(|s| !s.is_empty()),
            initialized: r.bool(12).unwrap_or(false),
            pin_protection: r.bool(7).unwrap_or(false),
            passphrase_protection: r.bool(8).unwrap_or(false),
            unlocked: r.bool(16).unwrap_or(false),
            capabilities: r.all_uint(30),
            session_id: r.bytes(35).map(<[u8]>::to_vec),
        })
    }

    pub fn can(&self, capability: u64) -> bool {
        self.capabilities.contains(&capability)
    }

    /// "Trezor Safe 3" or what the device calls itself.
    pub fn name(&self) -> String {
        match (&self.model, &self.label) {
            (_, Some(label)) => label.clone(),
            (Some(model), None) => format!("Trezor {model}"),
            (None, None) => "Trezor".into(),
        }
    }
}

/// A message type by its name in Trezor's protocol, for an error that
/// says what came where what was expected.
pub fn name(msg_type: u16) -> String {
    let known = match msg_type {
        INITIALIZE => "Initialize",
        SUCCESS => "Success",
        FAILURE => "Failure",
        FEATURES => "Features",
        PIN_MATRIX_REQUEST => "PinMatrixRequest",
        PIN_MATRIX_ACK => "PinMatrixAck",
        CANCEL => "Cancel",
        BUTTON_REQUEST => "ButtonRequest",
        BUTTON_ACK => "ButtonAck",
        PASSPHRASE_REQUEST => "PassphraseRequest",
        PASSPHRASE_ACK => "PassphraseAck",
        MONERO_TRANSACTION_INIT_REQUEST => "MoneroTransactionInitRequest",
        MONERO_TRANSACTION_INIT_ACK => "MoneroTransactionInitAck",
        MONERO_TRANSACTION_SET_INPUT_REQUEST => "MoneroTransactionSetInputRequest",
        MONERO_TRANSACTION_SET_INPUT_ACK => "MoneroTransactionSetInputAck",
        MONERO_TRANSACTION_INPUT_VINI_REQUEST => "MoneroTransactionInputViniRequest",
        MONERO_TRANSACTION_INPUT_VINI_ACK => "MoneroTransactionInputViniAck",
        MONERO_TRANSACTION_ALL_INPUTS_SET_REQUEST => "MoneroTransactionAllInputsSetRequest",
        MONERO_TRANSACTION_ALL_INPUTS_SET_ACK => "MoneroTransactionAllInputsSetAck",
        MONERO_TRANSACTION_SET_OUTPUT_REQUEST => "MoneroTransactionSetOutputRequest",
        MONERO_TRANSACTION_SET_OUTPUT_ACK => "MoneroTransactionSetOutputAck",
        MONERO_TRANSACTION_ALL_OUT_SET_REQUEST => "MoneroTransactionAllOutSetRequest",
        MONERO_TRANSACTION_ALL_OUT_SET_ACK => "MoneroTransactionAllOutSetAck",
        MONERO_TRANSACTION_SIGN_INPUT_REQUEST => "MoneroTransactionSignInputRequest",
        MONERO_TRANSACTION_SIGN_INPUT_ACK => "MoneroTransactionSignInputAck",
        MONERO_TRANSACTION_FINAL_REQUEST => "MoneroTransactionFinalRequest",
        MONERO_TRANSACTION_FINAL_ACK => "MoneroTransactionFinalAck",
        MONERO_KEY_IMAGE_EXPORT_INIT_REQUEST => "MoneroKeyImageExportInitRequest",
        MONERO_KEY_IMAGE_EXPORT_INIT_ACK => "MoneroKeyImageExportInitAck",
        MONERO_KEY_IMAGE_SYNC_STEP_REQUEST => "MoneroKeyImageSyncStepRequest",
        MONERO_KEY_IMAGE_SYNC_STEP_ACK => "MoneroKeyImageSyncStepAck",
        MONERO_KEY_IMAGE_SYNC_FINAL_REQUEST => "MoneroKeyImageSyncFinalRequest",
        MONERO_KEY_IMAGE_SYNC_FINAL_ACK => "MoneroKeyImageSyncFinalAck",
        MONERO_GET_ADDRESS => "MoneroGetAddress",
        MONERO_ADDRESS => "MoneroAddress",
        MONERO_GET_WATCH_KEY => "MoneroGetWatchKey",
        MONERO_WATCH_KEY => "MoneroWatchKey",
        _ => return format!("message {msg_type}"),
    };
    known.to_owned()
}

/// `Failure`: its code and text.
pub fn failure(data: &[u8]) -> (u64, String) {
    match Reader::parse(data) {
        Ok(r) => (r.uint(1).unwrap_or(0), r.string(2).unwrap_or_default()),
        Err(_) => (0, String::new()),
    }
}

/// `ButtonRequest.name`, or its code's name.
pub fn button_request(data: &[u8]) -> String {
    let Ok(r) = Reader::parse(data) else { return "Confirm on the device".into() };
    r.string(4).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("button {}", r.uint(1).unwrap_or(0)))
}

/// Wiped once sent: the PIN is in it.
pub fn pin_matrix_ack(pin: &str) -> Zeroizing<Vec<u8>> {
    let mut w = Writer::new();
    w.bytes(1, pin.as_bytes());
    Zeroizing::new(w.finish())
}

/// Wiped once sent: the passphrase is in it.
pub fn passphrase_ack(passphrase: Option<&str>, on_device: bool) -> Zeroizing<Vec<u8>> {
    let mut w = Writer::new();
    if let Some(text) = passphrase {
        w.bytes(1, text.as_bytes());
    }
    w.bool(3, on_device);
    Zeroizing::new(w.finish())
}

pub fn get_watch_key(network: Network) -> Vec<u8> {
    let mut w = Writer::new();
    path(&mut w, 1);
    w.uint(2, network_type(network));
    w.finish()
}

/// `MoneroWatchKey`: the private view key and the primary address.
pub fn watch_key(data: &[u8]) -> Result<([u8; 32], String), Malformed> {
    let r = Reader::parse(data)?;
    let key: [u8; 32] = r.bytes(1).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?;
    Ok((key, r.string(2).ok_or(Malformed)?))
}

pub fn get_address(network: Network, show: bool, minor: u32) -> Vec<u8> {
    let mut w = Writer::new();
    path(&mut w, 1);
    w.bool(2, show).uint(3, network_type(network)).uint(4, 0).uint(5, u64::from(minor));
    w.finish()
}

pub fn address(data: &[u8]) -> Result<String, Malformed> {
    Reader::parse(data)?.string(1).ok_or(Malformed)
}

/// An output whose key image the device is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub out_key: [u8; 32],
    pub tx_pub_key: [u8; 32],
    /// The extra key at the output's index, if its transaction had them.
    pub additional: Option<[u8; 32]>,
    pub index_in_tx: u64,
    pub subaddress_minor: u32,
}

impl Transfer {
    fn write(&self, w: &mut Writer) {
        w.bytes(1, &self.out_key).bytes(2, &self.tx_pub_key);
        if let Some(additional) = &self.additional {
            w.bytes(3, additional);
        }
        w.uint(4, self.index_in_tx).uint(5, 0).uint(6, u64::from(self.subaddress_minor));
    }
}

pub fn key_image_export_init(count: usize, hash: &[u8; 32], network: Network, minors: &[u32]) -> Vec<u8> {
    let mut w = Writer::new();
    w.uint(1, count as u64).bytes(2, hash);
    path(&mut w, 3);
    w.uint(4, network_type(network));
    let mut subs = Writer::new();
    subs.uint(1, 0);
    for minor in minors {
        subs.uint(2, u64::from(*minor));
    }
    w.message(5, &subs);
    w.finish()
}

pub fn key_image_sync_step(transfers: &[Transfer]) -> Vec<u8> {
    let mut w = Writer::new();
    for transfer in transfers {
        let mut t = Writer::new();
        transfer.write(&mut t);
        w.message(1, &t);
    }
    w.finish()
}

/// `MoneroKeyImageSyncStepAck`: each key image sealed, as (nonce, blob).
/// A key image as the device seals it: its nonce, then the ciphertext and tag.
pub type Sealed = (Vec<u8>, Vec<u8>);

pub fn key_image_sync_step_ack(data: &[u8]) -> Result<Vec<Sealed>, Malformed> {
    Reader::parse(data)?
        .all_messages(1)?
        .into_iter()
        .map(|k| Ok((k.bytes(1).ok_or(Malformed)?.to_vec(), k.bytes(3).ok_or(Malformed)?.to_vec())))
        .collect()
}

pub fn key_image_sync_final_ack(data: &[u8]) -> Result<[u8; 32], Malformed> {
    Reader::parse(data)?.bytes(1).ok_or(Malformed)?.try_into().map_err(|_| Malformed)
}

/// `MoneroTransactionDestinationEntry`: where money goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination {
    pub amount: u64,
    pub spend_public_key: [u8; 32],
    pub view_public_key: [u8; 32],
    pub is_subaddress: bool,
    pub is_integrated: bool,
}

impl Destination {
    pub fn write(&self) -> Writer {
        let mut addr = Writer::new();
        addr.bytes(1, &self.spend_public_key).bytes(2, &self.view_public_key);
        let mut w = Writer::new();
        w.uint(1, self.amount).message(2, &addr).bool(3, self.is_subaddress).bool(5, self.is_integrated);
        w
    }

    pub fn same_address(&self, other: &Destination) -> bool {
        self.spend_public_key == other.spend_public_key && self.view_public_key == other.view_public_key
    }
}

/// One member of an input's ring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingMember {
    /// Its index on the chain.
    pub index: u64,
    pub key: [u8; 32],
    pub commitment: [u8; 32],
}

/// `MoneroTransactionSourceEntry`: an input with its ring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub ring: Vec<RingMember>,
    pub real_output: u64,
    pub tx_pub_key: [u8; 32],
    pub additional: Option<[u8; 32]>,
    pub index_in_tx: u64,
    pub amount: u64,
    pub mask: [u8; 32],
    pub subaddress_minor: u32,
}

impl Source {
    /// Written with every member's index (`indices`) and key (`keys`), or
    /// only the real one's, as each step of the protocol wants.
    pub fn write(&self, indices: bool, keys: bool) -> Writer {
        let mut w = Writer::new();
        for (i, member) in self.ring.iter().enumerate() {
            let real = i as u64 == self.real_output;
            let mut out = Writer::new();
            if real || indices {
                out.uint(1, member.index);
            }
            if real || keys {
                let mut key = Writer::new();
                key.bytes(1, &member.key).bytes(2, &member.commitment);
                out.message(2, &key);
            }
            w.message(1, &out);
        }
        w.uint(2, self.real_output).bytes(3, &self.tx_pub_key);
        if let Some(additional) = &self.additional {
            w.bytes(4, additional);
        }
        w.uint(5, self.index_in_tx).uint(6, self.amount).bool(7, true).bytes(8, &self.mask).uint(10, u64::from(self.subaddress_minor));
        w
    }
}

/// `MoneroTransactionData`, the transaction as the device confirms it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionData {
    pub payment_id: Option<[u8; 8]>,
    pub outputs: Vec<Destination>,
    pub change: Destination,
    pub num_inputs: u32,
    pub mixin: u32,
    pub fee: u64,
    pub minor_indices: Vec<u32>,
    pub integrated_indices: Vec<u32>,
    pub hard_fork: u32,
}

/// Range proofs: padded Bulletproofs (3) of version 4 (Bulletproofs+), in
/// one batch; the device accepts nothing else.
pub const RSIG_PADDED_BULLETPROOF: u64 = 3;
pub const BP_VERSION_PLUS: u64 = 4;
/// `client_version` 4, as Monero's wallet says since firmware 2.5.2.
pub const CLIENT_VERSION: u64 = 4;

pub fn transaction_init(network: Network, data: &TransactionData) -> Vec<u8> {
    let mut tsx = Writer::new();
    tsx.uint(1, 1);
    tsx.bytes(2, data.payment_id.as_ref().map_or(&[][..], |id| &id[..]));
    tsx.uint(3, 0);
    for output in &data.outputs {
        tsx.message(4, &output.write());
    }
    tsx.message(5, &data.change.write());
    tsx.uint(6, u64::from(data.num_inputs)).uint(7, u64::from(data.mixin)).uint(8, data.fee).uint(9, 0);
    for minor in &data.minor_indices {
        tsx.uint(10, u64::from(*minor));
    }
    let mut rsig = Writer::new();
    rsig.uint(1, RSIG_PADDED_BULLETPROOF).uint(3, data.outputs.len() as u64).uint(7, BP_VERSION_PLUS);
    tsx.message(11, &rsig);
    for index in &data.integrated_indices {
        tsx.uint(12, u64::from(*index));
    }
    tsx.uint(13, CLIENT_VERSION).uint(14, u64::from(data.hard_fork));
    tsx.bytes(15, format!("fob {}|trezor", env!("CARGO_PKG_VERSION")).as_bytes());
    let mut w = Writer::new();
    w.uint(1, 0);
    path(&mut w, 2);
    w.uint(3, network_type(network)).message(4, &tsx);
    w.finish()
}

/// `MoneroTransactionInitAck`: an HMAC per output, and whether the range
/// proof is Fob's to make.
pub fn transaction_init_ack(data: &[u8]) -> Result<(Vec<Vec<u8>>, bool), Malformed> {
    let r = Reader::parse(data)?;
    let hmacs = r.all_bytes(1).into_iter().map(<[u8]>::to_vec).collect();
    let offload = r.message(2)?.and_then(|rsig| rsig.uint(2)).unwrap_or(0) != 0;
    Ok((hmacs, offload))
}

pub fn set_input(source: &Source) -> Vec<u8> {
    let mut w = Writer::new();
    w.message(1, &source.write(true, false));
    w.finish()
}

/// `MoneroTransactionSetInputAck`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputAck {
    pub vini: Vec<u8>,
    pub vini_hmac: Vec<u8>,
    pub pseudo_out: Vec<u8>,
    pub pseudo_out_hmac: Vec<u8>,
    pub pseudo_out_alpha: Vec<u8>,
    pub spend_key: Vec<u8>,
}

pub fn set_input_ack(data: &[u8]) -> Result<InputAck, Malformed> {
    let r = Reader::parse(data)?;
    let get = |field| r.bytes(field).map(<[u8]>::to_vec).ok_or(Malformed);
    Ok(InputAck { vini: get(1)?, vini_hmac: get(2)?, pseudo_out: get(3)?, pseudo_out_hmac: get(4)?, pseudo_out_alpha: get(5)?, spend_key: get(6)? })
}

pub fn input_vini(source: &Source, ack: &InputAck, original_index: u32) -> Vec<u8> {
    let mut w = Writer::new();
    w.message(1, &source.write(false, false)).bytes(2, &ack.vini).bytes(3, &ack.vini_hmac).uint(6, u64::from(original_index));
    w.finish()
}

pub fn set_output(destination: &Destination, hmac: &[u8], range_proof: Option<&[u8]>) -> Vec<u8> {
    let mut w = Writer::new();
    w.message(1, &destination.write()).bytes(2, hmac);
    if let Some(proof) = range_proof {
        let mut rsig = Writer::new();
        rsig.bytes(5, proof);
        w.message(3, &rsig).bool(4, true);
    }
    w.finish()
}

/// `MoneroTransactionSetOutputAck`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputAck {
    pub tx_out: Vec<u8>,
    pub vouti_hmac: Vec<u8>,
    /// The commitment's mask, when the range proof is Fob's to make.
    pub mask: Option<[u8; 32]>,
    /// The range proof, when the device made it.
    pub range_proof: Option<Vec<u8>>,
    pub out_pk: Vec<u8>,
    pub ecdh_info: Vec<u8>,
}

pub fn set_output_ack(data: &[u8]) -> Result<OutputAck, Malformed> {
    let r = Reader::parse(data)?;
    let get = |field| r.bytes(field).map(<[u8]>::to_vec).ok_or(Malformed);
    let rsig = r.message(3)?;
    let mask = rsig.as_ref().and_then(|rsig| rsig.bytes(4)).map(|m| m.try_into().map_err(|_| Malformed)).transpose()?;
    let range_proof = rsig.as_ref().and_then(|rsig| rsig.bytes(5)).filter(|p| !p.is_empty()).map(<[u8]>::to_vec);
    Ok(OutputAck { tx_out: get(1)?, vouti_hmac: get(2)?, mask, range_proof, out_pk: get(4)?, ecdh_info: get(5)? })
}

/// `MoneroTransactionAllOutSetAck`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllOutAck {
    pub extra: Vec<u8>,
    pub tx_prefix_hash: [u8; 32],
    pub fee: u64,
    pub rct_type: u64,
    pub full_message_hash: [u8; 32],
}

pub fn all_out_set_ack(data: &[u8]) -> Result<AllOutAck, Malformed> {
    let r = Reader::parse(data)?;
    let rv = r.message(4)?.ok_or(Malformed)?;
    Ok(AllOutAck {
        extra: r.bytes(1).ok_or(Malformed)?.to_vec(),
        tx_prefix_hash: r.bytes(2).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?,
        fee: rv.uint(1).ok_or(Malformed)?,
        rct_type: rv.uint(3).ok_or(Malformed)?,
        full_message_hash: r.bytes(5).ok_or(Malformed)?.try_into().map_err(|_| Malformed)?,
    })
}

pub fn sign_input(source: &Source, ack: &InputAck, original_index: u32) -> Vec<u8> {
    let mut w = Writer::new();
    w.message(1, &source.write(true, true))
        .bytes(2, &ack.vini)
        .bytes(3, &ack.vini_hmac)
        .bytes(4, &ack.pseudo_out)
        .bytes(5, &ack.pseudo_out_hmac)
        .bytes(6, &ack.pseudo_out_alpha)
        .bytes(7, &ack.spend_key)
        .uint(8, u64::from(original_index));
    w.finish()
}

/// `MoneroTransactionSignInputAck`: the sealed signature and the pseudo
/// output as finally balanced.
pub fn sign_input_ack(data: &[u8]) -> Result<(Vec<u8>, Option<[u8; 32]>), Malformed> {
    let r = Reader::parse(data)?;
    let pseudo_out = r.bytes(2).map(|p| p.try_into().map_err(|_| Malformed)).transpose()?;
    Ok((r.bytes(1).ok_or(Malformed)?.to_vec(), pseudo_out))
}

/// `MoneroTransactionFinalAck.opening_key`, which opens the signatures.
pub fn final_ack(data: &[u8]) -> Result<[u8; 32], Malformed> {
    Reader::parse(data)?.bytes(5).ok_or(Malformed)?.try_into().map_err(|_| Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_read_the_fields_a_device_sends() {
        let mut w = Writer::new();
        w.bytes(1, b"trezor.io").uint(2, 2).uint(3, 8).uint(4, 1).bool(7, true).bool(8, false).bytes(10, b"My Trezor").bool(12, true).bytes(21, b"Safe 3").uint(30, 1).uint(30, 9).bytes(35, &[1, 2, 3]);
        let f = Features::parse(&w.finish()).unwrap();
        assert_eq!(f.version, (2, 8, 1));
        assert!(f.can(CAPABILITY_MONERO) && !f.can(CAPABILITY_PASSPHRASE_ENTRY));
        assert_eq!(f.name(), "My Trezor");
        assert_eq!(f.session_id, Some(vec![1, 2, 3]));
        let unnamed = Features { label: None, model: Some("T".into()), ..Features::default() };
        assert_eq!(unnamed.name(), "Trezor T");
        assert_eq!(Features::default().name(), "Trezor");
    }

    #[test]
    fn the_path_and_network_go_into_every_request() {
        let r = Reader::parse(&get_watch_key(Network::Stagenet)).unwrap();
        assert_eq!(r.all_uint(1), vec![0x8000_002c, 0x8000_0080, 0x8000_0000]);
        assert_eq!(r.uint(2), Some(2));
        let r = Reader::parse(&get_address(Network::Mainnet, true, 5)).unwrap();
        assert_eq!((r.uint(3), r.bool(2), r.uint(4), r.uint(5)), (Some(0), Some(true), Some(0), Some(5)));
    }

    #[test]
    fn sources_name_the_ring_as_each_step_wants() {
        let source = Source {
            ring: vec![
                RingMember { index: 10, key: [1; 32], commitment: [2; 32] },
                RingMember { index: 14, key: [3; 32], commitment: [4; 32] },
            ],
            real_output: 1,
            tx_pub_key: [5; 32],
            additional: None,
            index_in_tx: 0,
            amount: 70,
            mask: [6; 32],
            subaddress_minor: 2,
        };
        let only_real = Reader::parse(source.write(false, false).as_bytes()).unwrap();
        let members = only_real.all_messages(1).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members[0].uint(1).is_none() && members[0].bytes(2).is_none(), "a decoy is left blank");
        assert_eq!(members[1].uint(1), Some(14));
        assert!(members[1].message(2).unwrap().is_some());
        let all = Reader::parse(source.write(true, true).as_bytes()).unwrap();
        let members = all.all_messages(1).unwrap();
        assert_eq!(members[0].uint(1), Some(10));
        assert_eq!(members[0].message(2).unwrap().unwrap().bytes(1), Some(&[1u8; 32][..]));
        assert_eq!((all.uint(2), all.bool(7), all.uint(10)), (Some(1), Some(true), Some(2)));
    }

    #[test]
    fn acks_round_trip_through_their_fields() {
        let mut w = Writer::new();
        w.bytes(1, b"vini").bytes(2, b"h1").bytes(3, b"po").bytes(4, b"h2").bytes(5, b"alpha").bytes(6, b"spend");
        let ack = set_input_ack(&w.finish()).unwrap();
        assert_eq!(ack.spend_key, b"spend");
        assert!(set_input_ack(&Writer::new().finish()).is_err());

        let mut rsig = Writer::new();
        rsig.bytes(4, &[7; 32]);
        let mut w = Writer::new();
        w.bytes(1, b"out").bytes(2, b"h").message(3, &rsig).bytes(4, &[0; 64]).bytes(5, &[0; 8]);
        let ack = set_output_ack(&w.finish()).unwrap();
        assert_eq!(ack.mask, Some([7; 32]));
        assert_eq!(ack.range_proof, None);

        let mut rv = Writer::new();
        rv.uint(1, 900).bytes(2, &[1; 32]).uint(3, 6);
        let mut w = Writer::new();
        w.bytes(1, b"extra").bytes(2, &[2; 32]).message(4, &rv).bytes(5, &[3; 32]);
        let ack = all_out_set_ack(&w.finish()).unwrap();
        assert_eq!((ack.fee, ack.rct_type, ack.extra.as_slice()), (900, 6, &b"extra"[..]));

        let mut w = Writer::new();
        w.bytes(1, b"sig").bytes(2, &[9; 32]);
        assert_eq!(sign_input_ack(&w.finish()).unwrap(), (b"sig".to_vec(), Some([9; 32])));

        let mut w = Writer::new();
        w.bytes(1, b"cout").bytes(5, &[4; 32]);
        assert_eq!(final_ack(&w.finish()).unwrap(), [4; 32]);
    }

    #[test]
    fn transaction_init_carries_what_the_device_checks() {
        let dest = |amount, spend| Destination { amount, spend_public_key: [spend; 32], view_public_key: [1; 32], is_subaddress: false, is_integrated: false };
        let data = TransactionData {
            payment_id: None,
            outputs: vec![dest(5, 2), dest(3, 3)],
            change: dest(3, 3),
            num_inputs: 2,
            mixin: 15,
            fee: 1,
            minor_indices: vec![0, 1],
            integrated_indices: vec![],
            hard_fork: 16,
        };
        let r = Reader::parse(&transaction_init(Network::Mainnet, &data)).unwrap();
        assert_eq!(r.all_uint(2).len(), 3);
        let tsx = r.message(4).unwrap().unwrap();
        assert_eq!(tsx.all_messages(4).unwrap().len(), 2);
        assert_eq!(tsx.bytes(2), Some(&[][..]), "no payment id is sent as empty bytes, as Monero does");
        assert_eq!((tsx.uint(6), tsx.uint(7), tsx.uint(8), tsx.uint(13), tsx.uint(14)), (Some(2), Some(15), Some(1), Some(4), Some(16)));
        let rsig = tsx.message(11).unwrap().unwrap();
        assert_eq!((rsig.uint(1), rsig.all_uint(3), rsig.uint(7)), (Some(3), vec![2], Some(4)));
        let (hmacs, offload) = transaction_init_ack(&{
            let mut w = Writer::new();
            w.bytes(1, b"a").bytes(1, b"b");
            let mut rsig = Writer::new();
            rsig.uint(2, 1);
            w.message(2, &rsig);
            w.finish()
        })
        .unwrap();
        assert_eq!((hmacs.len(), offload), (2, true));
    }

    #[test]
    fn key_image_sync_messages() {
        let t = Transfer { out_key: [1; 32], tx_pub_key: [2; 32], additional: Some([3; 32]), index_in_tx: 4, subaddress_minor: 5 };
        let r = Reader::parse(&key_image_sync_step(&[t.clone(), t])).unwrap();
        let tdis = r.all_messages(1).unwrap();
        assert_eq!(tdis.len(), 2);
        assert_eq!((tdis[0].bytes(3), tdis[0].uint(4), tdis[0].uint(5), tdis[0].uint(6)), (Some(&[3u8; 32][..]), Some(4), Some(0), Some(5)));
        let r = Reader::parse(&key_image_export_init(2, &[8; 32], Network::Mainnet, &[0, 3])).unwrap();
        assert_eq!(r.uint(1), Some(2));
        let subs = r.all_messages(5).unwrap();
        assert_eq!(subs[0].all_uint(2), vec![0, 3]);
        let mut w = Writer::new();
        let mut k = Writer::new();
        k.bytes(1, &[0; 12]).bytes(3, &[1; 112]);
        w.message(1, &k);
        assert_eq!(key_image_sync_step_ack(&w.finish()).unwrap(), vec![(vec![0; 12], vec![1; 112])]);
    }
}
