//! Connect to Fob, version 1: another app (Monokular's Till, or any app that
//! speaks this) asks Fob for view access to one of its wallets, and Fob
//! answers with everything the app needs to watch it, sealed so only the
//! app that asked can open it (decision 417).
//!
//! 1. **Request.** The app makes a one-time X25519 key pair and a random
//!    16-byte `state`, and opens (or shows as a QR code)
//!    `fob://connect?v=1&app=Till&name=Corner%20Caf%C3%A9&device=Corner%20Caf%C3%A9%20iPad&net=mainnet&pk=…&state=…&want=view,node,prices&cb=…`.
//!    `pk` and `state` are base64url without padding; `cb` is optional (the
//!    two-device way has none).
//! 2. **Consent** in Fob, which makes a labelled account for the app.
//! 3. **Answer.** Fob makes its own one-time key pair and seals the
//!    [`Grant`] as JSON with ChaCha20-Poly1305 under
//!    HKDF-SHA256(salt = state, ikm = X25519 shared secret,
//!    info = `"fob connect v1"` ‖ pk ‖ epk). The key is used once, so the
//!    nonce is all zeros. It opens `<cb>#v=1&state=…&epk=…&box=…` (the
//!    fragment stays out of a web server's logs), or with no `cb`
//!    `fob://connected#…`, which is also what its QR code holds. A no is
//!    `<cb>#v=1&state=…&error=declined`.
//! 4. **The app** checks `state`, opens the box and forgets its key.
//!
//! Both screens show the same [`Identicon`] of the request's key, so a
//! request swapped by someone nearby shows a different pattern. Every byte
//! of this is pinned by the vectors in `vectors/connect-v1.json`, which
//! [`check_vectors`] runs, in Fob's tests and in any app's.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::{Aead, Payload}};
use hkdf::Hkdf;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use fob_monero::network::Network;

use crate::qr::{self, QrMatrix};

/// The protocol's version, `v=` in every request and answer.
pub const VERSION: u32 = 1;
/// Where a request goes.
pub const REQUEST_PREFIX: &str = "fob://connect?";
/// Where an answer goes when the request gave no callback (the two-device
/// way, by QR code).
pub const QR_CALLBACK: &str = "fob://connected";
/// The longest app, wallet-name and device text a request may carry, in
/// characters.
pub const MAX_TEXT: usize = 64;
/// The longest callback, in bytes.
pub const MAX_CALLBACK: usize = 512;

const INFO: &[u8] = b"fob connect v1";
const AAD: &[u8] = b"fob://connect v=1";
const IDENTICON_DOMAIN: &[u8] = b"fob connect identicon v1";

/// What a value in a request or answer has percent-encoded: everything
/// but RFC 3986's unreserved characters.
const VALUE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Why a request or answer can't be read, made or opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// Not a Connect to Fob request or answer at all.
    NotConnect,
    /// A version this Fob doesn't speak.
    Version(String),
    /// A field missing, repeated or not as it must be, by name.
    Field(&'static str),
    /// The network isn't one Fob has.
    Network(String),
    /// The answer is for another request.
    WrongState,
    /// The box doesn't open with this request's key: changed, or for
    /// another app.
    Sealed,
    /// What's inside the box isn't a grant.
    Payload,
    /// Too long for a QR code.
    TooBig,
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::NotConnect => f.write_str("not a Connect to Fob code"),
            ConnectError::Version(v) => write!(f, "Connect to Fob version {v} isn't known"),
            ConnectError::Field(name) => write!(f, "the request's {name} isn't right"),
            ConnectError::Network(n) => write!(f, "{n} isn't a network Fob has"),
            ConnectError::WrongState => f.write_str("this answer is for another request"),
            ConnectError::Sealed => f.write_str("this answer doesn't open"),
            ConnectError::Payload => f.write_str("this answer holds nothing Fob gives"),
            ConnectError::TooBig => f.write_str("too long for a QR code"),
        }
    }
}

impl std::error::Error for ConnectError {}

pub type Result<T> = std::result::Result<T, ConnectError>;

/// What the app asks for besides view access, which is always asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Want {
    /// Fob's node or light wallet server, so the app needs no setup.
    pub node: bool,
    /// Fob's price provider and currency.
    pub prices: bool,
}

impl Want {
    fn text(self) -> String {
        let mut items = vec!["view"];
        if self.node {
            items.push("node");
        }
        if self.prices {
            items.push("prices");
        }
        items.join(",")
    }

    /// From `want=`: "view" must be there; anything not known is left out,
    /// for a later version's sake.
    fn parse(text: &str) -> Result<Want> {
        let items: Vec<&str> = text.split(',').collect();
        if !items.contains(&"view") {
            return Err(ConnectError::Field("want"));
        }
        Ok(Want { node: items.contains(&"node"), prices: items.contains(&"prices") })
    }
}

/// A request for view access, as `fob://connect?…` carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The asking app's name, as its screens and Fob's say it ("Till").
    pub app: String,
    /// What the app would call its account ("Corner Café"): Fob labels the
    /// account "<app> (<name>)", editable on the consent screen.
    pub name: String,
    /// The device asking ("Corner Café iPad").
    pub device: String,
    pub network: Network,
    /// The app's one-time X25519 public key.
    pub pk: [u8; 32],
    /// Random, made with the key: the answer must carry it back.
    pub state: [u8; 16],
    pub want: Want,
    /// Where the answer goes, if anywhere: a link the app opens. `None` for
    /// the two-device way, where Fob shows the answer as a QR code.
    pub callback: Option<String>,
}

fn clean_text(text: &str, field: &'static str, may_be_empty: bool) -> Result<String> {
    let text = text.trim();
    if (!may_be_empty && text.is_empty()) || text.chars().count() > MAX_TEXT || text.chars().any(char::is_control) {
        return Err(ConnectError::Field(field));
    }
    Ok(text.to_owned())
}

fn check_callback(cb: &str) -> Result<()> {
    let scheme_ok = cb.split_once(':').is_some_and(|(scheme, rest)| {
        !rest.is_empty() && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    if !scheme_ok || cb.len() > MAX_CALLBACK || cb.contains('#') || cb.chars().any(|c| c.is_control() || c == ' ') {
        return Err(ConnectError::Field("cb"));
    }
    Ok(())
}

fn network_named(name: &str) -> Result<Network> {
    match name {
        "mainnet" => Ok(Network::Mainnet),
        "stagenet" => Ok(Network::Stagenet),
        other => Err(ConnectError::Network(other.to_owned())),
    }
}

fn decode_fixed<const N: usize>(text: &str, field: &'static str) -> Result<[u8; N]> {
    B64.decode(text).ok().and_then(|b| b.try_into().ok()).ok_or(ConnectError::Field(field))
}

/// `key=value&…` read into pairs, each value percent-decoded; a key given
/// twice is refused, so no reader can be shown one value and act on another.
fn pairs(query: &str) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = vec![];
    for part in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let value = percent_decode_str(value).decode_utf8().map_err(|_| ConnectError::NotConnect)?.into_owned();
        if out.iter().any(|(k, _)| k == key) {
            return Err(ConnectError::Field(static_name(key)));
        }
        out.push((key.to_owned(), value));
    }
    Ok(out)
}

/// A field's name as an error names it.
fn static_name(key: &str) -> &'static str {
    ["v", "app", "name", "device", "net", "pk", "state", "want", "cb", "epk", "box", "error"].into_iter().find(|k| *k == key).unwrap_or("query")
}

fn get<'a>(pairs: &'a [(String, String)], key: &'static str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn need<'a>(pairs: &'a [(String, String)], key: &'static str) -> Result<&'a str> {
    get(pairs, key).ok_or(ConnectError::Field(key))
}

fn check_version(pairs: &[(String, String)]) -> Result<()> {
    match need(pairs, "v")? {
        "1" => Ok(()),
        other => Err(ConnectError::Version(other.to_owned())),
    }
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, VALUE).to_string()
}

impl Request {
    /// The request as a link, its fields in this order:
    /// `fob://connect?v=1&app=…&name=…&device=…&net=…&pk=…&state=…&want=…[&cb=…]`.
    pub fn to_uri(&self) -> String {
        let mut uri = format!(
            "{REQUEST_PREFIX}v={VERSION}&app={}&name={}&device={}&net={}&pk={}&state={}&want={}",
            encode(&self.app),
            encode(&self.name),
            encode(&self.device),
            self.network.name(),
            B64.encode(self.pk),
            B64.encode(self.state),
            self.want.text(),
        );
        if let Some(cb) = &self.callback {
            uri.push_str("&cb=");
            uri.push_str(&encode(cb));
        }
        uri
    }

    /// Reads a request, from a link or a scanned code. Fields it doesn't
    /// know are left out; a missing, repeated or malformed one is refused.
    pub fn parse(text: &str) -> Result<Request> {
        let query = text.trim().strip_prefix(REQUEST_PREFIX).ok_or(ConnectError::NotConnect)?;
        if query.contains('#') {
            return Err(ConnectError::NotConnect);
        }
        let pairs = pairs(query)?;
        check_version(&pairs)?;
        let callback = get(&pairs, "cb").map(str::to_owned);
        if let Some(cb) = &callback {
            check_callback(cb)?;
        }
        Ok(Request {
            app: clean_text(need(&pairs, "app")?, "app", false)?,
            name: clean_text(get(&pairs, "name").unwrap_or(""), "name", true)?,
            device: clean_text(get(&pairs, "device").unwrap_or(""), "device", true)?,
            network: network_named(need(&pairs, "net")?)?,
            pk: decode_fixed(need(&pairs, "pk")?, "pk")?,
            state: decode_fixed(need(&pairs, "state")?, "state")?,
            want: Want::parse(need(&pairs, "want")?)?,
            callback,
        })
    }

    /// Whether `text` looks like a Connect to Fob request, for a scanner
    /// deciding which screen a code opens.
    pub fn looks_like(text: &str) -> bool {
        text.trim().starts_with(REQUEST_PREFIX)
    }

    /// The account's label Fob suggests: "Till (Corner Café)", or the app's
    /// name alone when the request names nothing.
    pub fn label(&self) -> String {
        if self.name.is_empty() { self.app.clone() } else { format!("{} ({})", self.app, self.name) }
    }

    /// The identicon both screens show for this request.
    pub fn identicon(&self) -> Identicon {
        identicon(&self.pk)
    }

    /// Where the answer goes: the callback, or [`QR_CALLBACK`].
    fn answer_base(&self) -> &str {
        self.callback.as_deref().unwrap_or(QR_CALLBACK)
    }

    /// The request as a QR code, for the two-device way.
    pub fn qr(&self) -> Result<QrMatrix> {
        qr::encode(&self.to_uri()).map_err(|_| ConnectError::TooBig)
    }
}

/// A node shared with the app: its URL, whether its self-signed certificate
/// is accepted, and the pinned key's fingerprint (hex) if so.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedNode {
    pub url: String,
    #[serde(default)]
    pub self_signed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

/// Fob's price source: its provider's name ("Kraken") and the currency's
/// code ("AUD").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedPrices {
    pub provider: String,
    pub currency: String,
}

/// What Fob hands over, sealed. The view key is wiped when it goes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// The wallet's primary address.
    pub address: String,
    /// The private view key, hex.
    pub view_key: String,
    /// "mainnet" or "stagenet".
    pub network: String,
    /// The account Fob made for the app, and its label.
    pub account: u32,
    pub label: String,
    /// Fob's tip when it answered: nothing before it is the app's.
    pub restore_height: u64,
    /// Fob's own id for the wallet, random, so the app can say "Connected to
    /// Fob" and Fob can tell a reconnect.
    pub wallet_id: String,
    /// The wallet's name in Fob ("Corner Café").
    #[serde(default)]
    pub wallet_name: String,
    /// Fob's node, when it was asked for and shared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<SharedNode>,
    /// Fob's light wallet server instead, when the wallet syncs through one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<SharedNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prices: Option<SharedPrices>,
}

impl Grant {
    /// The network it's for.
    pub fn network(&self) -> Result<Network> {
        network_named(&self.network)
    }
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grant").field("address", &self.address).field("account", &self.account).field("label", &self.label).finish_non_exhaustive()
    }
}

impl Drop for Grant {
    fn drop(&mut self) {
        self.view_key.zeroize();
    }
}

/// The key a box is sealed under: HKDF-SHA256 of the shared secret, salted
/// with the request's state, bound to both public keys.
fn box_key(shared: &[u8; 32], state: &[u8; 16], pk: &[u8; 32], epk: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(state), shared);
    let mut info = INFO.to_vec();
    info.extend_from_slice(pk);
    info.extend_from_slice(epk);
    let mut key = Zeroizing::new([0u8; 32]);
    hk.expand(&info, &mut *key).expect("32 bytes is a valid HKDF length");
    key
}

fn shared_secret(secret: &StaticSecret, public: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
    let shared = secret.diffie_hellman(&PublicKey::from(*public));
    // A low-order key gives a secret anyone knows.
    if !shared.was_contributory() {
        return Err(ConnectError::Field("pk"));
    }
    Ok(Zeroizing::new(shared.to_bytes()))
}

/// Fob's answer: `grant` sealed for `request`, under a new one-time key.
pub fn seal(request: &Request, grant: &Grant) -> Result<String> {
    let mut ephemeral = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(&mut *ephemeral);
    seal_with(request, grant, &ephemeral)
}

/// [`seal`] with the one-time secret given: for the vectors.
pub fn seal_with(request: &Request, grant: &Grant, ephemeral: &[u8; 32]) -> Result<String> {
    let secret = StaticSecret::from(*ephemeral);
    let epk = PublicKey::from(&secret).to_bytes();
    let shared = shared_secret(&secret, &request.pk)?;
    let key = box_key(&shared, &request.state, &request.pk, &epk);
    let plain = Zeroizing::new(serde_json::to_vec(grant).map_err(|_| ConnectError::Payload)?);
    let sealed = ChaCha20Poly1305::new((&*key).into())
        .encrypt(Nonce::from_slice(&[0; 12]), Payload { msg: &plain, aad: AAD })
        .map_err(|_| ConnectError::Payload)?;
    Ok(format!("{}#v={VERSION}&state={}&epk={}&box={}", request.answer_base(), B64.encode(request.state), B64.encode(epk), B64.encode(sealed)))
}

/// Fob's answer when the person said no.
pub fn decline(request: &Request) -> String {
    format!("{}#v={VERSION}&state={}&error=declined", request.answer_base(), B64.encode(request.state))
}

/// An answer as a QR code, for the two-device way.
pub fn answer_qr(answer: &str) -> Result<QrMatrix> {
    qr::encode(answer).map_err(|_| ConnectError::TooBig)
}

/// What came back.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Granted(Box<Grant>),
    /// The person said no (`error=declined`, or any other error Fob sends).
    Declined,
}

/// The app's side: its one-time key and the request made with it. The key
/// is wiped when this goes; keep it only until the answer is opened.
pub struct Asker {
    secret: StaticSecret,
    request: Request,
}

impl Asker {
    /// A new request with a fresh key and state.
    pub fn new(app: &str, name: &str, device: &str, network: Network, want: Want, callback: Option<&str>) -> Result<Asker> {
        let mut secret = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(&mut *secret);
        let mut state = [0u8; 16];
        OsRng.fill_bytes(&mut state);
        Asker::with(&secret, state, app, name, device, network, want, callback)
    }

    /// [`new`](Asker::new) with the key and state given: for the vectors,
    /// and for an app that keeps a request across a restart.
    #[allow(clippy::too_many_arguments)]
    pub fn with(secret: &[u8; 32], state: [u8; 16], app: &str, name: &str, device: &str, network: Network, want: Want, callback: Option<&str>) -> Result<Asker> {
        let secret = StaticSecret::from(*secret);
        if let Some(cb) = callback {
            check_callback(cb)?;
        }
        let request = Request {
            app: clean_text(app, "app", false)?,
            name: clean_text(name, "name", true)?,
            device: clean_text(device, "device", true)?,
            network,
            pk: PublicKey::from(&secret).to_bytes(),
            state,
            want,
            callback: callback.map(str::to_owned),
        };
        Ok(Asker { secret, request })
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    /// The one-time secret, to keep across a restart (in the platform's
    /// secure storage) and give back to [`with`](Asker::with).
    pub fn secret(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.secret.to_bytes())
    }

    /// Opens Fob's answer, a link or a scanned code: checks it's for this
    /// request and opens the box.
    pub fn open(&self, answer: &str) -> Result<Answer> {
        let (_, fragment) = answer.trim().split_once('#').ok_or(ConnectError::NotConnect)?;
        let pairs = pairs(fragment)?;
        check_version(&pairs)?;
        let state: [u8; 16] = decode_fixed(need(&pairs, "state")?, "state")?;
        if state != self.request.state {
            return Err(ConnectError::WrongState);
        }
        if get(&pairs, "error").is_some() {
            return Ok(Answer::Declined);
        }
        let epk: [u8; 32] = decode_fixed(need(&pairs, "epk")?, "epk")?;
        let sealed = B64.decode(need(&pairs, "box")?).map_err(|_| ConnectError::Field("box"))?;
        let shared = shared_secret(&self.secret, &epk).map_err(|_| ConnectError::Sealed)?;
        let key = box_key(&shared, &self.request.state, &self.request.pk, &epk);
        let plain = Zeroizing::new(
            ChaCha20Poly1305::new((&*key).into())
                .decrypt(Nonce::from_slice(&[0; 12]), Payload { msg: &sealed, aad: AAD })
                .map_err(|_| ConnectError::Sealed)?,
        );
        let grant: Grant = serde_json::from_slice(&plain).map_err(|_| ConnectError::Payload)?;
        if grant.network().ok() != Some(self.request.network) {
            return Err(ConnectError::Payload);
        }
        Ok(Answer::Granted(Box::new(grant)))
    }
}

/// Whether `text` looks like an answer to a Connect to Fob request.
pub fn looks_like_answer(text: &str) -> bool {
    text.split_once('#').is_some_and(|(_, f)| f.starts_with("v=") && f.contains("state="))
}

/// Two colours an identicon is drawn in: `ink` for set cells, `paper`
/// behind, as `#rrggbb`. The same in light and dark themes, so both
/// devices show the same picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    pub ink: &'static str,
    pub paper: &'static str,
}

/// The identicon's palettes, by index.
pub const PALETTE: [Swatch; 8] = [
    Swatch { ink: "#c2410c", paper: "#ffedd5" },
    Swatch { ink: "#1d4ed8", paper: "#dbeafe" },
    Swatch { ink: "#15803d", paper: "#dcfce7" },
    Swatch { ink: "#7e22ce", paper: "#f3e8ff" },
    Swatch { ink: "#be123c", paper: "#ffe4e6" },
    Swatch { ink: "#0f766e", paper: "#ccfbf1" },
    Swatch { ink: "#a16207", paper: "#fef9c3" },
    Swatch { ink: "#334155", paper: "#e2e8f0" },
];

/// A symmetric 5×5 pattern in two colours, from a hash of a request's
/// public key: what both screens draw so a person can see they match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identicon {
    /// Row by row, top first; `true` is drawn in ink. Each row reads the
    /// same both ways.
    pub cells: [[bool; 5]; 5],
    /// Which of [`PALETTE`] to draw it in.
    pub palette: u8,
}

impl Identicon {
    pub fn swatch(&self) -> Swatch {
        PALETTE[self.palette as usize % PALETTE.len()]
    }

    /// Rows as text, `#` for ink and `.` for paper: how the vectors write it.
    pub fn rows(&self) -> Vec<String> {
        self.cells.iter().map(|row| row.iter().map(|&c| if c { '#' } else { '.' }).collect()).collect()
    }
}

/// The identicon of a request's public key: SHA-256 of
/// `"fob connect identicon v1"` ‖ pk; bit `3r + c` (least significant bit
/// of each byte first, from byte 0) sets row r's column c and its mirror
/// 4 − c, for columns 0 to 2; byte 4, modulo 8, picks the palette.
pub fn identicon(pk: &[u8; 32]) -> Identicon {
    let hash = Sha256::new().chain_update(IDENTICON_DOMAIN).chain_update(pk).finalize();
    let bit = |i: usize| hash[i / 8] >> (i % 8) & 1 == 1;
    let mut cells = [[false; 5]; 5];
    for (r, row) in cells.iter_mut().enumerate() {
        for c in 0..3 {
            let on = bit(3 * r + c);
            row[c] = on;
            row[4 - c] = on;
        }
    }
    Identicon { cells, palette: hash[4] % PALETTE.len() as u8 }
}

/// The vectors every implementation of version 1 must reproduce: fixed
/// keys and state, the exact request link, sealed answer and identicon.
pub const VECTORS: &str = include_str!("../vectors/connect-v1.json");

#[derive(Deserialize)]
struct Vectors {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    app_secret: String,
    state: String,
    app: String,
    #[serde(rename = "wallet_name")]
    name_field: String,
    device: String,
    network: String,
    want: Vec<String>,
    callback: Option<String>,
    fob_secret: String,
    grant: serde_json::Value,
    request: String,
    pk: String,
    answer: String,
    declined: String,
    identicon: Vec<String>,
    palette: u8,
}

fn hex32(text: &str) -> std::result::Result<[u8; 32], String> {
    let bytes = (0..text.len()).step_by(2).map(|i| text.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect::<Option<Vec<u8>>>();
    bytes.and_then(|b| b.try_into().ok()).ok_or_else(|| format!("not 32 bytes of hex: {text}"))
}

/// Runs `text` (as [`VECTORS`]) through this implementation; the number of
/// cases on success, or what first differed.
pub fn check_vectors(text: &str) -> std::result::Result<usize, String> {
    let vectors: Vectors = serde_json::from_str(text).map_err(|e| format!("vectors: {e}"))?;
    if vectors.version != VERSION {
        return Err(format!("vectors for version {}", vectors.version));
    }
    for case in &vectors.cases {
        let fail = |what: &str, got: &dyn std::fmt::Debug, want: &dyn std::fmt::Debug| Err(format!("{}: {what}: got {got:?}, want {want:?}", case.name));
        let state: [u8; 16] = B64.decode(&case.state).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| format!("{}: state", case.name))?;
        let want = Want { node: case.want.iter().any(|w| w == "node"), prices: case.want.iter().any(|w| w == "prices") };
        let network = network_named(&case.network).map_err(|e| e.to_string())?;
        let asker = Asker::with(&hex32(&case.app_secret)?, state, &case.app, &case.name_field, &case.device, network, want, case.callback.as_deref()).map_err(|e| e.to_string())?;
        if B64.encode(asker.request.pk) != case.pk {
            return fail("pk", &B64.encode(asker.request.pk), &case.pk);
        }
        let uri = asker.request.to_uri();
        if uri != case.request {
            return fail("request", &uri, &case.request);
        }
        let parsed = Request::parse(&case.request).map_err(|e| format!("{}: parsing the request: {e}", case.name))?;
        if parsed != asker.request {
            return fail("parsed request", &parsed, &asker.request);
        }
        let grant: Grant = serde_json::from_value(case.grant.clone()).map_err(|e| format!("{}: grant: {e}", case.name))?;
        let answer = seal_with(&parsed, &grant, &hex32(&case.fob_secret)?).map_err(|e| e.to_string())?;
        if answer != case.answer {
            return fail("answer", &answer, &case.answer);
        }
        match asker.open(&case.answer) {
            Ok(Answer::Granted(opened)) if *opened == grant => {}
            other => return fail("opened", &other, &grant),
        }
        let declined = decline(&parsed);
        if declined != case.declined || asker.open(&declined) != Ok(Answer::Declined) {
            return fail("declined", &declined, &case.declined);
        }
        let icon = parsed.identicon();
        if icon.rows() != case.identicon || icon.palette != case.palette {
            return fail("identicon", &(icon.rows(), icon.palette), &(&case.identicon, case.palette));
        }
    }
    Ok(vectors.cases.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asker() -> Asker {
        Asker::with(&[1; 32], [2; 16], "Till", "Corner Café", "Corner Café iPad", Network::Mainnet, Want { node: true, prices: true }, Some("till://connected")).unwrap()
    }

    fn grant() -> Grant {
        Grant {
            address: "4ABC".into(),
            view_key: "11".repeat(32),
            network: "mainnet".into(),
            account: 2,
            label: "Till (Corner Café)".into(),
            restore_height: 3_500_000,
            wallet_id: "w1".into(),
            wallet_name: "Corner Café".into(),
            node: Some(SharedNode { url: "https://node.example:18089".into(), self_signed: false, pin: None }),
            light: None,
            prices: Some(SharedPrices { provider: "Kraken".into(), currency: "AUD".into() }),
        }
    }

    #[test]
    fn the_vectors_pass() {
        assert_eq!(check_vectors(VECTORS), Ok(3));
    }

    #[test]
    fn a_request_reads_back_as_written() {
        let asker = asker();
        let uri = asker.request().to_uri();
        assert!(uri.starts_with("fob://connect?v=1&app=Till&name=Corner%20Caf%C3%A9&device=Corner%20Caf%C3%A9%20iPad&net=mainnet&pk="));
        assert!(uri.contains("&want=view,node,prices&cb=till%3A%2F%2Fconnected"));
        assert_eq!(Request::parse(&uri).unwrap(), *asker.request());
        assert!(Request::looks_like(&uri));
        assert_eq!(asker.request().label(), "Till (Corner Café)");
    }

    #[test]
    fn a_sealed_grant_opens_only_for_its_request() {
        let asker = asker();
        let answer = seal(asker.request(), &grant()).unwrap();
        assert!(answer.starts_with("till://connected#v=1&state="));
        assert!(looks_like_answer(&answer));
        assert_eq!(asker.open(&answer), Ok(Answer::Granted(Box::new(grant()))));
        // Another request's key can't open it, even with the same state.
        let other = Asker::with(&[3; 32], [2; 16], "Till", "", "", Network::Mainnet, Want::default(), None).unwrap();
        assert_eq!(other.open(&answer), Err(ConnectError::Sealed));
        // Another state is another request.
        let later = Asker::with(&[1; 32], [9; 16], "Till", "", "", Network::Mainnet, Want::default(), None).unwrap();
        assert_eq!(later.open(&answer), Err(ConnectError::WrongState));
        // A changed box doesn't open.
        let (head, sealed) = answer.rsplit_once("box=").unwrap();
        let mut bytes = B64.decode(sealed).unwrap();
        bytes[0] ^= 1;
        assert_eq!(asker.open(&format!("{head}box={}", B64.encode(bytes))), Err(ConnectError::Sealed));
        // Each answer has its own key.
        assert_ne!(seal(asker.request(), &grant()).unwrap(), answer);
    }

    #[test]
    fn a_grant_for_another_network_is_refused() {
        let asker = asker();
        let mut stagenet = grant();
        stagenet.network = "stagenet".into();
        assert_eq!(asker.open(&seal(asker.request(), &stagenet).unwrap()), Err(ConnectError::Payload));
    }

    #[test]
    fn no_and_no_callback() {
        let asker = Asker::with(&[1; 32], [2; 16], "Till", "Kiosk", "", Network::Stagenet, Want::default(), None).unwrap();
        let declined = decline(asker.request());
        assert_eq!(declined, format!("fob://connected#v=1&state={}&error=declined", B64.encode([2u8; 16])));
        assert_eq!(asker.open(&declined), Ok(Answer::Declined));
        let uri = asker.request().to_uri();
        assert!(uri.ends_with("&want=view"), "{uri}");
        assert!(asker.request().qr().unwrap().size > 0);
    }

    #[test]
    fn bad_requests_are_refused() {
        let good = asker().request().to_uri();
        let bad = |from: &str, to: &str| Request::parse(&good.replace(from, to));
        assert_eq!(bad("v=1", "v=2"), Err(ConnectError::Version("2".into())));
        assert_eq!(bad("net=mainnet", "net=testnet"), Err(ConnectError::Network("testnet".into())));
        assert_eq!(bad("want=view,node,prices", "want=node"), Err(ConnectError::Field("want")));
        assert_eq!(bad("app=Till", "app="), Err(ConnectError::Field("app")));
        assert_eq!(bad("app=Till", "app=Till&app=Evil"), Err(ConnectError::Field("app")));
        assert_eq!(bad("cb=till%3A%2F%2Fconnected", "cb=nothing"), Err(ConnectError::Field("cb")));
        assert_eq!(bad("cb=till%3A%2F%2Fconnected", "cb=till%3A%2F%2Fx%23y"), Err(ConnectError::Field("cb")));
        assert_eq!(Request::parse("monero:4abc"), Err(ConnectError::NotConnect));
        let state = &good[good.find("state=").unwrap() + 6..good.find("&want").unwrap()];
        assert_eq!(bad(state, "AAAA"), Err(ConnectError::Field("state")));
        // Fields from a later version are left out.
        assert!(bad("&want=", "&later=1&want=").is_ok());
        // A key of low order gives a secret anyone knows: refused.
        let mut low = asker().request().clone();
        low.pk = [0; 32];
        assert_eq!(seal(&low, &grant()), Err(ConnectError::Field("pk")));
    }

    #[test]
    fn an_identicon_is_symmetric_and_follows_the_key() {
        for seed in 0..50u8 {
            let icon = identicon(&[seed; 32]);
            for row in icon.cells {
                assert_eq!(row[0], row[4]);
                assert_eq!(row[1], row[3]);
            }
            assert!((icon.palette as usize) < PALETTE.len());
        }
        assert_ne!(identicon(&[1; 32]), identicon(&[2; 32]));
        assert_eq!(identicon(&[1; 32]), identicon(&[1; 32]));
    }

    #[test]
    fn a_grant_s_debug_shows_no_view_key() {
        assert!(!format!("{:?}", grant()).contains("1111"));
    }
}
