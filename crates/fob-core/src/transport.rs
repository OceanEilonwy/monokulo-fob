//! How Fob talks to a node: pooled keep-alive HTTP(S) connections, with
//! requests running side by side.
//!
//! monero-oxide's own `simple-request` transport opens a new connection for
//! every request and lets only one run at a time, which makes catching up
//! about one block a second. Here every request gets a pooled connection of
//! its own, so the fetcher can keep several batches in flight.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use monero_daemon_rpc::{HttpTransport, MoneroDaemon};
use monero_wallet::interface::InterfaceError;

/// How long a node may go quiet mid-request. There's no limit on a whole
/// request: decoy selection downloads the chain's output distribution,
/// megabytes from a slow public node.
const QUIET: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Transport {
    client: reqwest::Client,
    base: Arc<str>,
    requests: Arc<AtomicU64>,
    /// Bytes the block fetcher has taken from the node, for the link speed.
    received: Arc<AtomicU64>,
    /// What the last TLS handshake made of the node's certificate.
    certificate: Seen,
}

pub type Daemon = MoneroDaemon<Transport>;

/// Why a node's certificate didn't verify.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertProblem {
    /// Signed by no authority the phone knows: the node's own, usually.
    /// A private authority's looks the same and is accepted the same way.
    SelfSigned,
    Expired,
    NotYetValid,
    /// Made out to another host name.
    OtherName,
}

impl CertProblem {
    /// "self-signed", for "self-signed, not accepted".
    pub fn words(self) -> &'static str {
        match self {
            CertProblem::SelfSigned => "self-signed",
            CertProblem::Expired => "expired",
            CertProblem::NotYetValid => "not valid yet",
            CertProblem::OtherName => "for another address",
        }
    }

    fn of(error: &rustls::Error) -> CertProblem {
        use rustls::CertificateError as C;
        match error {
            rustls::Error::InvalidCertificate(C::Expired | C::ExpiredContext { .. }) => CertProblem::Expired,
            rustls::Error::InvalidCertificate(C::NotValidYet | C::NotValidYetContext { .. }) => CertProblem::NotYetValid,
            rustls::Error::InvalidCertificate(C::NotValidForName | C::NotValidForNameContext { .. }) => CertProblem::OtherName,
            // Unknown issuer, or (openssl's usual self-signed certificate)
            // an authority's certificate used as the server's own.
            _ => CertProblem::SelfSigned,
        }
    }
}

/// The node's certificate as last checked: `None` before any TLS handshake
/// (and always over plain HTTP).
type Seen = Arc<std::sync::Mutex<Option<Result<(), CertProblem>>>>;

/// The phone's usual check of a node's certificate (Mozilla's roots, as
/// reqwest's own), noting what it found. For a node marked to accept
/// self-signed certificates, any certificate passes: the connection is
/// still encrypted, and the handshake still proves the node holds the
/// certificate's key, but nothing says whose key that is. Only that one
/// node's connections are made with it; price providers have their own
/// clients with the usual check.
///
/// With any certificate accepted, the handshake's signature is checked
/// against the certificate's key read on its own, not through webpki:
/// monerod's own self-signed certificates are version 1, which webpki
/// won't read at all, so the usual signature check would refuse them.
#[derive(Debug)]
struct Verifier {
    usual: Arc<rustls::client::WebPkiServerVerifier>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
    accept_any: bool,
    seen: Seen,
}

impl Verifier {
    /// The handshake's signature over `message` by `cert`'s key, read
    /// without judging the certificate (any version). TLS 1.2 says less of
    /// the algorithm, so each that fits the scheme and the key is tried.
    fn signed_by_key(&self, message: &[u8], cert: &[u8], dss: &rustls::DigitallySignedStruct, tls13: bool) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let (spki, algorithm, key) = public_key(cert).ok_or(rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
        if tls13 {
            return rustls::crypto::verify_tls13_signature_with_raw_key(message, &spki.into(), dss, &self.algorithms);
        }
        let unadvertised = rustls::Error::PeerMisbehaved(rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme);
        let candidates = self.algorithms.mapping.iter().find(|(scheme, _)| *scheme == dss.scheme).ok_or(unadvertised)?.1;
        candidates
            .iter()
            .filter(|a| a.public_key_alg_id().as_ref() == algorithm)
            .find(|a| a.verify_signature(key, message, dss.signature()).is_ok())
            .map(|_| rustls::client::danger::HandshakeSignatureValid::assertion())
            .ok_or(rustls::Error::InvalidCertificate(rustls::CertificateError::BadSignature))
    }
}

/// A certificate's public key, read without judging the certificate: its
/// SubjectPublicKeyInfo whole, the algorithm's DER (inside its SEQUENCE,
/// as webpki names algorithms) and the key's bytes.
pub(crate) fn public_key(cert: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let (cert, _) = der(cert, 0x30)?;
    let (mut tbs, _) = der(cert, 0x30)?;
    // [0] version: only in version 2 and 3 certificates.
    if tbs.first() == Some(&0xa0) {
        tbs = der(tbs, 0xa0)?.1;
    }
    // Serial number, signature algorithm, issuer, validity, subject.
    for tag in [0x02, 0x30, 0x30, 0x30, 0x30] {
        tbs = der(tbs, tag)?.1;
    }
    let (inside, _) = der(tbs, 0x30)?;
    let spki = &tbs[..tbs.len() - der(tbs, 0x30)?.1.len()];
    let (algorithm, rest) = der(inside, 0x30)?;
    let (bits, _) = der(rest, 0x03)?;
    // A key is whole bytes: no unused bits.
    Some((spki, algorithm, bits.strip_prefix(&[0])?))
}

/// The DER element tagged `tag` at the start of `bytes`: its contents and
/// what follows it.
fn der(bytes: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (&found, bytes) = bytes.split_first()?;
    let (&first, bytes) = bytes.split_first()?;
    if found != tag {
        return None;
    }
    let (len, bytes) = match first {
        short @ 0..0x80 => (short as usize, bytes),
        long => {
            let n = (long & 0x7f) as usize;
            if !(1..=4).contains(&n) || bytes.len() < n {
                return None;
            }
            (bytes[..n].iter().fold(0usize, |len, &b| len << 8 | b as usize), &bytes[n..])
        }
    };
    (bytes.len() >= len).then(|| bytes.split_at(len))
}

impl rustls::client::danger::ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        intermediates: &[rustls::pki_types::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        ocsp_response: &[u8],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let verdict = self.usual.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now);
        *self.seen.lock().expect("certificate lock") = Some(verdict.as_ref().map(|_| ()).map_err(CertProblem::of));
        match verdict {
            Err(_) if self.accept_any => Ok(rustls::client::danger::ServerCertVerified::assertion()),
            verdict => verdict,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        if self.accept_any {
            return self.signed_by_key(message, cert, dss, false);
        }
        self.usual.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        if self.accept_any {
            return self.signed_by_key(message, cert, dss, true);
        }
        self.usual.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.usual.supported_verify_schemes()
    }
}

/// TLS for one node: the usual check, or any certificate with `accept_any`.
fn tls(accept_any: bool, seen: Seen) -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = Arc::new(rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
    let usual = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider.clone()).build().map_err(|e| e.to_string())?;
    let provider_algorithms = provider.signature_verification_algorithms;
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Verifier { usual, algorithms: provider_algorithms, accept_any, seen }))
        .with_no_client_auth();
    // As reqwest offers by default.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

impl Transport {
    /// A transport to the node at `url` (`http://host:port` or `https://…`;
    /// user:pass@ in the URL is sent as Basic auth). With `self_signed`,
    /// an https node's certificate isn't verified (see [`Verifier`]).
    /// An onion node is refused here too, whatever path led to it (an
    /// older wallet file, say): there's no Tor to reach it through.
    pub fn new(url: &str, self_signed: bool) -> Result<Transport, InterfaceError> {
        crate::node::reachable(url).map_err(InterfaceError::InternalError)?;
        let certificate = Seen::default();
        let tls = tls(self_signed, certificate.clone()).map_err(|e| InterfaceError::InternalError(format!("TLS: {e}")))?;
        let client = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .read_timeout(QUIET)
            .connect_timeout(Duration::from_secs(15))
            // monerod closes idle keep-alive connections; reusing one it has
            // dropped fails the request, so idle ones go after a few seconds.
            .pool_idle_timeout(Duration::from_secs(8))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .map_err(|e| InterfaceError::InternalError(format!("HTTP client: {e}")))?;
        Ok(Transport {
            client,
            base: url.trim_end_matches('/').into(),
            requests: Arc::new(AtomicU64::new(0)),
            received: Arc::new(AtomicU64::new(0)),
            certificate,
        })
    }

    /// The node's certificate at the last handshake: verified, or why not
    /// (accepted anyway for a node marked self-signed). `None` before one.
    pub fn certificate(&self) -> Option<Result<(), CertProblem>> {
        *self.certificate.lock().expect("certificate lock")
    }

    /// Requests made so far, for tests and benchmarks.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Bytes of blocks fetched so far: what the link speed measures.
    pub fn received(&self) -> u64 {
        self.received.load(Ordering::Relaxed)
    }

    pub(crate) fn count_received(&self, bytes: usize) {
        self.received.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

impl HttpTransport for Transport {
    fn post(
        &self,
        route: &str,
        body: Vec<u8>,
        response_size_limit: Option<usize>,
    ) -> impl Send + Future<Output = Result<Vec<u8>, InterfaceError>> {
        let url = format!("{}/{route}", self.base);
        let json = !route.ends_with(".bin");
        let client = self.client.clone();
        let requests = self.requests.clone();
        async move {
            // A pooled connection the node closed while idle fails before the
            // node sees the request; Monero's RPC calls are safe to repeat
            // (re-relaying a transaction included), so send it again.
            let mut attempt = 0;
            loop {
                let request = client.post(&url).body(body.clone());
                let request = if json { request.header("Content-Type", "application/json") } else { request };
                match Self::send(request, &requests, response_size_limit).await {
                    Err(InterfaceError::InterfaceError(e)) if attempt < 2 && crate::fetch::stale(&e) => attempt += 1,
                    other => return other,
                }
            }
        }
    }
}

impl Transport {
    async fn send(request: reqwest::RequestBuilder, requests: &AtomicU64, response_size_limit: Option<usize>) -> Result<Vec<u8>, InterfaceError> {
        requests.fetch_add(1, Ordering::Relaxed);
        let err = |e: reqwest::Error| {
            // reqwest's own message is only "error sending request"; the cause is below it.
            let e = e.without_url();
            let mut text = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(cause) = source {
                text.push_str(": ");
                text.push_str(&cause.to_string());
                source = cause.source();
            }
            InterfaceError::InterfaceError(text)
        };
        let mut response = request.send().await.map_err(err)?;
        if !response.status().is_success() {
            return Err(InterfaceError::InterfaceError(format!("node answered {}", response.status())));
        }
        // Read it all before the connection goes back to the pool.
        let mut body = Vec::with_capacity(response.content_length().unwrap_or(1024) as usize);
        while let Some(chunk) = response.chunk().await.map_err(err)? {
            body.extend_from_slice(&chunk);
            if response_size_limit.is_some_and(|limit| body.len() > limit) {
                return Err(InterfaceError::InvalidInterface("node's response was too large".into()));
            }
        }
        Ok(body)
    }
}
