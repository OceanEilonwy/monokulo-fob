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

/// Begins the words of a request that failed before any connection to the
/// node was made, so nothing was sent.
const NOT_CONNECTED: &str = "not connected: ";

/// Whether a request that failed with `error` (an interface error's text,
/// debug or not) never reached the node: no connection was made, or the
/// node turned it away unread with a 4xx status. Anything else may have
/// reached it, a timeout or a line dropped mid-answer included.
pub fn never_reached(error: &str) -> bool {
    error.contains(NOT_CONNECTED) || error.contains("node answered 4")
}

#[derive(Clone)]
pub struct Transport {
    client: reqwest::Client,
    base: Arc<str>,
    requests: Arc<AtomicU64>,
    /// Bytes the block fetcher has taken from the node, for the link speed.
    received: Arc<AtomicU64>,
    /// What the last TLS handshake made of the node's certificate.
    certificate: Seen,
    /// The key a node marked self-signed is held to: see [`Verifier`].
    pin: Pin,
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
    /// Made out to another host name. webpki only gets as far as the name
    /// for a certificate an authority the phone trusts signed (a node's own
    /// fails as self-signed first): usually a node reached by its IP address
    /// with the certificate for its name. Accepted only for a node marked
    /// self-signed, and then held to its key like a self-signed one, as
    /// anyone can get an authority's certificate for some name of their own.
    OtherName,
    /// Not the key this node had when its certificate was accepted: someone
    /// else answering, or the node given a new one. Never accepted; choosing
    /// the node again accepts the new one.
    Changed,
    /// The handshake isn't signed by the certificate's key: whoever
    /// answered doesn't hold it. Never accepted.
    BadSignature,
    /// Broken, revoked, or signed wrongly by its authority. Never accepted.
    Invalid,
}

impl CertProblem {
    /// Whether a node marked to accept a self-signed certificate takes one
    /// with this problem: only what a node's own certificate can have, or an
    /// authority's for its name (see [`OtherName`](CertProblem::OtherName)),
    /// each held to the key first seen.
    pub fn acceptable(self) -> bool {
        !matches!(self, CertProblem::BadSignature | CertProblem::Invalid | CertProblem::Changed)
    }

    pub(crate) fn of(error: &rustls::Error) -> CertProblem {
        use rustls::CertificateError as C;
        let rustls::Error::InvalidCertificate(error) = error else { return CertProblem::Invalid };
        match error {
            C::Expired | C::ExpiredContext { .. } => CertProblem::Expired,
            C::NotValidYet | C::NotValidYetContext { .. } => CertProblem::NotYetValid,
            C::NotValidForName | C::NotValidForNameContext { .. } => CertProblem::OtherName,
            C::UnknownIssuer => CertProblem::SelfSigned,
            // webpki's own errors, which rustls passes on only in words:
            // openssl's usual self-signed certificate (an authority's used
            // as the server's own), and monerod's (version 1).
            C::Other(other) if matches!(other.to_string().as_str(), "CaUsedAsEndEntity" | "UnsupportedCertVersion") => CertProblem::SelfSigned,
            _ => CertProblem::Invalid,
        }
    }
}

/// The node's certificate as last checked: `None` before any TLS handshake
/// (and always over plain HTTP).
type Seen = Arc<std::sync::Mutex<Option<Result<(), CertProblem>>>>;

/// The fingerprint of the key a self-signed node is held to: the SHA3-256 of
/// its certificate's SubjectPublicKeyInfo. `None` until the first handshake
/// gives one, when it isn't known from before.
type Pin = Arc<std::sync::Mutex<Option<[u8; 32]>>>;

/// A certificate's fingerprint, as [`Pin`] keeps it.
fn fingerprint(cert: &[u8]) -> Option<[u8; 32]> {
    use sha3::{Digest, Sha3_256};
    let (spki, _, _) = public_key(cert)?;
    Some(Sha3_256::digest(spki).into())
}

/// The phone's usual check of a node's certificate (Mozilla's roots, as
/// reqwest's own), noting what it found. For a node marked to accept
/// self-signed certificates, a certificate passes that a node's own could be
/// (self-signed or expired; not a broken one), or an authority's for another
/// name: the connection is still encrypted, and the handshake still proves
/// the node holds the certificate's key. Nothing says whose key that is the
/// first time, so it's trusted on first use: the key is remembered, and a
/// later handshake with another is refused, whoever signed it, so whoever
/// gets between the phone and the node afterwards can't pass as it. Only
/// that one node's connections are made with it; price providers have their
/// own clients with the usual check.
///
/// With any certificate accepted, the handshake's signature is checked
/// against the certificate's key read on its own, not through webpki:
/// monerod's own self-signed certificates are version 1, which webpki
/// won't read at all, so the usual signature check would refuse them.
#[derive(Debug)]
struct Verifier {
    /// webpki's, but for tests standing in for an authority's verdict.
    usual: Arc<dyn rustls::client::danger::ServerCertVerifier>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
    accept_any: bool,
    seen: Seen,
    pin: Pin,
}

impl Verifier {
    /// Holds `cert` to the key pinned, pinning it if none is yet.
    fn pinned(&self, cert: &[u8]) -> Result<(), CertProblem> {
        let Some(key) = fingerprint(cert) else { return Err(CertProblem::Invalid) };
        let mut pin = self.pin.lock().expect("pin lock");
        match *pin {
            Some(pinned) if pinned != key => Err(CertProblem::Changed),
            Some(_) => Ok(()),
            None => {
                *pin = Some(key);
                Ok(())
            }
        }
    }

    fn note(&self, verdict: Result<(), CertProblem>) {
        *self.seen.lock().expect("certificate lock") = Some(verdict);
    }

    /// The handshake's signature check, a failure noted as the
    /// certificate's problem: whoever answered doesn't hold its key.
    fn signed(&self, signed: Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        if signed.is_err() {
            self.note(Err(CertProblem::BadSignature));
        }
        signed
    }

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
        let problem = verdict.as_ref().err().map(CertProblem::of);
        self.note(problem.map_or(Ok(()), Err));
        let passes = problem.is_none_or(|problem| self.accept_any && problem.acceptable());
        // The pin is checked whoever signed the certificate: one an authority
        // signed for the name passes the usual check, and a node that was
        // pinned must not be passed for by whoever can get one. A node marked
        // self-signed is pinned to the first key seen, an authority's or not.
        let pinning = self.accept_any || self.pin.lock().expect("pin lock").is_some();
        if !passes || !pinning {
            return verdict;
        }
        match self.pinned(end_entity) {
            Ok(()) => Ok(rustls::client::danger::ServerCertVerified::assertion()),
            Err(changed) => {
                self.note(Err(changed));
                Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let signed = if self.accept_any { self.signed_by_key(message, cert, dss, false) } else { self.usual.verify_tls12_signature(message, cert, dss) };
        self.signed(signed)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let signed = if self.accept_any { self.signed_by_key(message, cert, dss, true) } else { self.usual.verify_tls13_signature(message, cert, dss) };
        self.signed(signed)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.usual.supported_verify_schemes()
    }
}

/// TLS for one node: the usual check, or any certificate with `accept_any`.
fn tls(accept_any: bool, seen: Seen, pin: Pin) -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = Arc::new(rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
    let usual = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider.clone()).build().map_err(|e| e.to_string())?;
    let provider_algorithms = provider.signature_verification_algorithms;
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Verifier { usual, algorithms: provider_algorithms, accept_any, seen, pin }))
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
        Self::pinned(url, self_signed, None)
    }

    /// [`new`](Transport::new), holding a self-signed node to the key `pin`
    /// fingerprints, from an earlier connection; with none, to the key of
    /// the first handshake, which [`pin`](Transport::pin) then gives.
    pub fn pinned(url: &str, self_signed: bool, pin: Option<[u8; 32]>) -> Result<Transport, InterfaceError> {
        crate::node::reachable(url).map_err(|e| InterfaceError::InternalError(match e {
            crate::error::EndpointProblem::Onion => crate::node::ONION_REFUSED.to_owned(),
            other => format!("{other:?}"),
        }))?;
        let certificate = Seen::default();
        let pin = Pin::new(std::sync::Mutex::new(pin));
        let tls = tls(self_signed, certificate.clone(), pin.clone()).map_err(|e| InterfaceError::InternalError(format!("TLS: {e}")))?;
        let client = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .read_timeout(QUIET)
            .connect_timeout(Duration::from_secs(15))
            // monerod closes idle keep-alive connections; reusing one it has
            // dropped fails the request, so idle ones go after a few seconds.
            .pool_idle_timeout(Duration::from_secs(8))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            // monerod never redirects; following one would take this node's
            // certificate rule, and a name lookup, to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| InterfaceError::InternalError(format!("HTTP client: {e}")))?;
        Ok(Transport {
            client,
            base: url.trim_end_matches('/').into(),
            requests: Arc::new(AtomicU64::new(0)),
            received: Arc::new(AtomicU64::new(0)),
            certificate,
            pin,
        })
    }

    /// The fingerprint of the key a self-signed node is held to, once known:
    /// for the wallet to keep, and hold the node to next time.
    pub fn pin(&self) -> Option<[u8; 32]> {
        *self.pin.lock().expect("pin lock")
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
        let err = |e: reqwest::Error| InterfaceError::InterfaceError(failure_text(e));
        let mut response = request.send().await.map_err(err)?;
        if !response.status().is_success() {
            return Err(InterfaceError::InterfaceError(format!("node answered {}", response.status())));
        }
        // Read it all before the connection goes back to the pool. The length
        // the node gives is only its word: a huge one would have the phone
        // set aside memory it hasn't got, so it's refused past the limit and
        // never trusted for more than a megabyte up front.
        let said = response.content_length();
        if let (Some(said), Some(limit)) = (said, response_size_limit)
            && said > limit as u64
        {
            return Err(InterfaceError::InvalidInterface("node's response was too large".into()));
        }
        let mut body = Vec::with_capacity(said.unwrap_or(1024).min(1 << 20) as usize);
        while let Some(chunk) = response.chunk().await.map_err(err)? {
            body.extend_from_slice(&chunk);
            if response_size_limit.is_some_and(|limit| body.len() > limit) {
                return Err(InterfaceError::InvalidInterface("node's response was too large".into()));
            }
        }
        Ok(body)
    }

    /// POSTs JSON to `route` and gives the status and the body whatever the
    /// status, up to `limit` bytes: for a light wallet server, whose API
    /// says things with its statuses (403, 501, 409). Sent again, as
    /// [`post`](HttpTransport::post) does, when a pooled connection had gone
    /// stale before the server saw it; every call of that API is safe to
    /// repeat. A failure's words never hold the body, which holds a view key.
    pub(crate) async fn post_json(&self, route: &str, body: Vec<u8>, limit: usize) -> Result<(u16, Vec<u8>), String> {
        let url = format!("{}/{route}", self.base);
        let mut attempt = 0;
        loop {
            self.requests.fetch_add(1, Ordering::Relaxed);
            let sent = self.client.post(&url).header("Content-Type", "application/json").body(body.clone()).send().await;
            let mut response = match sent {
                Ok(response) => response,
                Err(e) => {
                    let text = failure_text(e);
                    if attempt < 2 && crate::fetch::stale(&text) {
                        attempt += 1;
                        continue;
                    }
                    return Err(text);
                }
            };
            let status = response.status().as_u16();
            if response.content_length().is_some_and(|said| said > limit as u64) {
                return Err("the server's answer was too large".into());
            }
            let mut out = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(failure_text)? {
                out.extend_from_slice(&chunk);
                if out.len() > limit {
                    return Err("the server's answer was too large".into());
                }
            }
            return Ok((status, out));
        }
    }
}

/// A failed request's words: reqwest's own message is only "error sending
/// request", so the causes below it follow. Never the request itself.
fn failure_text(e: reqwest::Error) -> String {
    let e = e.without_url();
    let mut text = if e.is_connect() { NOT_CONNECTED.to_owned() } else { String::new() };
    text.push_str(&e.to_string());
    let mut source = std::error::Error::source(&e);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::canned::{CERT, MONEROD_CERT};
    use rustls::client::danger::ServerCertVerifier;

    /// webpki, saying what an authority would of any certificate: valid for
    /// the name, or `problem`, as no test certificate is an authority's.
    #[derive(Debug)]
    struct Authority {
        webpki: Arc<rustls::client::WebPkiServerVerifier>,
        problem: Option<rustls::CertificateError>,
    }

    impl ServerCertVerifier for Authority {
        fn verify_server_cert(
            &self,
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: &rustls::pki_types::ServerName<'_>,
            _: &[u8],
            _: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            match &self.problem {
                Some(problem) => Err(rustls::Error::InvalidCertificate(problem.clone())),
                None => Ok(rustls::client::danger::ServerCertVerified::assertion()),
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            self.webpki.verify_tls12_signature(message, cert, dss)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &rustls::pki_types::CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            self.webpki.verify_tls13_signature(message, cert, dss)
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            self.webpki.supported_verify_schemes()
        }
    }

    fn verifier(problem: Option<rustls::CertificateError>, accept_any: bool, pin: Option<&[u8]>) -> Verifier {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = Arc::new(rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
        let webpki = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider.clone()).build().unwrap();
        Verifier {
            usual: Arc::new(Authority { webpki, problem }),
            algorithms: provider.signature_verification_algorithms,
            accept_any,
            seen: Seen::default(),
            pin: Pin::new(std::sync::Mutex::new(pin.and_then(fingerprint))),
        }
    }

    impl Verifier {
        /// Whether `cert` is accepted, or what was noted of it when it isn't.
        fn shown(&self, cert: &[u8]) -> Result<(), CertProblem> {
            let name = rustls::pki_types::ServerName::try_from("node.example").unwrap();
            let cert = rustls::pki_types::CertificateDer::from(cert.to_vec());
            match self.verify_server_cert(&cert, &[], &name, &[], rustls::pki_types::UnixTime::now()) {
                Ok(_) => Ok(()),
                Err(_) => Err(self.seen.lock().unwrap().expect("noted").expect_err("noted as a problem")),
            }
        }
    }

    /// A pinned node is held to its key whoever signed its certificate: an
    /// authority's for the name, from whoever got between, is refused too
    /// (review of audit #13).
    #[test]
    fn a_pin_holds_even_for_an_authoritys_certificate() {
        let pinned = verifier(None, true, Some(CERT));
        assert_eq!(pinned.shown(CERT), Ok(()));
        assert_eq!(pinned.shown(MONEROD_CERT), Err(CertProblem::Changed));
        // Not marked self-signed now, but pinned from before: held all the same.
        let unmarked = verifier(None, false, Some(CERT));
        assert_eq!(unmarked.shown(MONEROD_CERT), Err(CertProblem::Changed));
        assert_eq!(unmarked.shown(CERT), Ok(()));
    }

    /// A node marked self-signed is pinned to the first key it answers with,
    /// an authority's certificate or its own; a node not marked, with no pin,
    /// has only the usual check, and nothing is pinned.
    #[test]
    fn a_node_marked_self_signed_is_pinned_to_its_first_key_whoever_signed_it() {
        let marked = verifier(None, true, None);
        assert_eq!(marked.shown(MONEROD_CERT), Ok(()));
        assert_eq!(*marked.pin.lock().unwrap(), fingerprint(MONEROD_CERT));
        assert_eq!(marked.shown(CERT), Err(CertProblem::Changed));
        let usual = verifier(None, false, None);
        assert_eq!(usual.shown(CERT), Ok(()));
        assert_eq!(usual.shown(MONEROD_CERT), Ok(()));
        assert_eq!(*usual.pin.lock().unwrap(), None);
    }

    /// An authority's certificate for the node's name, reached by its
    /// address: accepted only with the mark, and then pinned like a node's own.
    #[test]
    fn a_certificate_for_another_name_is_accepted_only_marked_and_pinned() {
        let other = || Some(rustls::CertificateError::NotValidForName);
        assert_eq!(verifier(other(), false, None).shown(CERT), Err(CertProblem::OtherName));
        let marked = verifier(other(), true, None);
        assert_eq!(marked.shown(CERT), Ok(()));
        assert_eq!(*marked.pin.lock().unwrap(), fingerprint(CERT));
        assert_eq!(marked.shown(MONEROD_CERT), Err(CertProblem::Changed));
        assert_eq!(verifier(Some(rustls::CertificateError::BadSignature), true, None).shown(CERT), Err(CertProblem::Invalid), "a broken one still isn't");
    }
}
