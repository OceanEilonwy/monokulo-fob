//! A node on 127.0.0.1 for tests, here and in the wallet's.

use std::sync::Arc;


use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The height the node says it has.
pub const HEIGHT: u64 = 1_792_005;

/// A self-signed certificate for 127.0.0.1 and localhost (CA:TRUE, as
/// openssl makes them by default), valid until 2126, and its key.
pub const CERT: &[u8] = include_bytes!("self-signed.crt.der");
pub const KEY: &[u8] = include_bytes!("self-signed.key.der");
/// A certificate as monerod makes its own: version 1, no subject or
/// issuer, no extensions (which webpki won't read at all), and its key.
pub const MONEROD_CERT: &[u8] = include_bytes!("monerod-v1.crt.der");
pub const MONEROD_KEY: &[u8] = include_bytes!("monerod-v1.key.der");

/// How [`canned_node`] answers: plain HTTP, or TLS with a certificate.
#[derive(Clone, Copy)]
pub enum Canned {
    Http,
    /// [`CERT`] (or monerod's), over TLS 1.3 or (with `tls12`) 1.2 only.
    Tls { monerod: bool, tls12: bool },
    /// The certificate of one and the key of the other: someone answering
    /// with a certificate they don't hold the key of.
    WrongKey { monerod: bool, tls12: bool },
    /// Plain HTTP, every request sent on to the node on this port.
    Redirect(u16),
}

/// A stagenet node on 127.0.0.1 that answers get_info and the last block
/// header, and sends blocks that aren't blocks; over TLS with [`CERT`]
/// when `tls`. Gives its port.
pub async fn canned_node(tls: bool) -> u16 {
    canned(if tls { Canned::Tls { monerod: false, tls12: false } } else { Canned::Http }).await
}

/// [`canned_node`], answering as `how` says.
pub async fn canned(how: Canned) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let redirect = match how {
        Canned::Redirect(to) => Some(to),
        _ => None,
    };
    let acceptor = match how {
        Canned::Http | Canned::Redirect(_) => None,
        Canned::Tls { monerod, tls12 } | Canned::WrongKey { monerod, tls12 } => {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let versions: &[&rustls::SupportedProtocolVersion] = if tls12 { &[&rustls::version::TLS12] } else { rustls::DEFAULT_VERSIONS };
            let (cert, key) = match (how, monerod) {
                (Canned::Tls { .. }, true) => (MONEROD_CERT, MONEROD_KEY),
                (Canned::Tls { .. }, false) => (CERT, KEY),
                (_, true) => (MONEROD_CERT, KEY),
                (_, false) => (CERT, MONEROD_KEY),
            };
            // Served as it is: rustls's own check of a certificate against its
            // key won't read a version 1 one either.
            let key = provider.key_provider.load_private_key(rustls::pki_types::PrivatePkcs8KeyDer::from(key.to_vec()).into()).unwrap();
            let certified = rustls::sign::CertifiedKey::new(vec![cert.to_vec().into()], key);
            let config = rustls::ServerConfig::builder_with_provider(provider)
                .with_protocol_versions(versions)
                .unwrap()
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(rustls::sign::SingleCertAndKey::from(certified)));
            Some(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
        }
    };
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                match acceptor {
                    // A client that refuses the certificate hangs up here.
                    Some(acceptor) => {
                        if let Ok(stream) = acceptor.accept(stream).await {
                            answer(stream, None).await;
                        }
                    }
                    None => answer(stream, redirect).await,
                }
            });
        }
    });
    port
}

/// Answers one request, or sends it on to the node at `redirect`, then closes.
async fn answer(mut stream: impl AsyncRead + AsyncWrite + Unpin, redirect: Option<u16>) {
    let mut request = vec![];
    let mut buf = [0; 4096];
    let (head, body_len) = loop {
        let Ok(n) = stream.read(&mut buf).await else { return };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&buf[..n]);
        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
            let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
            break (head, end + 4 + len);
        }
    };
    while request.len() < body_len {
        let Ok(n) = stream.read(&mut buf).await else { return };
        if n == 0 {
            return;
        }
        request.extend_from_slice(&buf[..n]);
    }
    let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let body = match path.as_str() {
        "/get_info" => format!(r#"{{"height":{HEIGHT},"target_height":0,"nettype":"stagenet","status":"OK"}}"#),
        "/json_rpc" => format!(r#"{{"result":{{"block_header":{{"height":{},"timestamp":{}}}}}}}"#, HEIGHT - 1, now - 30),
        _ => "not epee".into(),
    };
    let response = match redirect {
        Some(to) => format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:{to}{path}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        None => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()),
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}
