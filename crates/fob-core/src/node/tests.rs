use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::*;

/// A node that answers with canned results; `None` never answers.
#[derive(Clone)]
struct Fake {
    info: Option<Result<Info, Failure>>,
    last_block: Option<Result<(u64, u64), Failure>>,
    /// Blocks and bytes, taking a second.
    blocks: Option<Result<(u64, u64), Failure>>,
    certificate: Option<Result<(), CertProblem>>,
}

const NOW: u64 = 1_793_500_000;
const HEIGHT: u64 = 1_792_005;

fn good() -> Fake {
    Fake {
        info: Some(Ok(Info { nettype: "stagenet".into(), height: HEIGHT, target_height: 0 })),
        last_block: Some(Ok((HEIGHT - 1, NOW - 75))),
        blocks: Some(Ok((31, 1_800_000))),
        certificate: None,
    }
}

impl Probe for Fake {
    async fn info(&self) -> Result<Info, Failure> {
        match self.info.clone() {
            Some(info) => info,
            None => std::future::pending().await,
        }
    }

    async fn last_block(&self) -> Result<(u64, u64), Failure> {
        match self.last_block.clone() {
            Some(last) => last,
            None => std::future::pending().await,
        }
    }

    async fn blocks(&self, start: u64, end: u64) -> Result<(u64, u64), Failure> {
        assert!(start >= 1 && start <= end && end - start < SAMPLE, "{start}..={end}");
        match self.blocks.clone() {
            Some(blocks) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
                blocks
            }
            None => std::future::pending().await,
        }
    }

    fn certificate(&self) -> Option<Result<(), CertProblem>> {
        self.certificate
    }
}

fn node(url: &str) -> Setting {
    Setting { url: url.into(), self_signed: false }
}

async fn checked(probe: &Fake, url: &str) -> Report {
    run(probe, Report::pending(Network::Stagenet, &node(url)), 0, NOW, &|_| {}).await
}

fn rows(report: &Report) -> Vec<(&str, &str, Mark)> {
    report.rows.iter().map(|r| (r.title(), r.result.as_str(), r.mark)).collect()
}

#[tokio::test(start_paused = true)]
async fn a_good_node_passes_every_check() {
    let report = checked(&good(), "node.example:38081").await;
    assert_eq!(
        rows(&report),
        [
            ("Answers", "0 ms", Mark::Pass),
            ("Right network", "stagenet", Mark::Pass),
            ("Up to date", "block 1,792,004 · 1 min ago", Mark::Pass),
            ("Sends blocks to Fob", "31 blocks/s", Mark::Pass),
        ]
    );
    // The link speed on a line of its own, never beside the blocks; only there.
    let seconds: Vec<_> = report.rows.iter().map(|r| r.second.as_deref()).collect();
    assert_eq!(seconds, [None, None, None, Some("1.8 MB/s")]);
    assert_eq!(report.rows[3].label(), "Sends blocks to Fob: 31 blocks/s, 1.8 MB/s");
    assert_eq!((report.verdict(), report.usable(), report.address.as_str()), ("Working", Usable::Use, "node.example:38081"));
    assert_eq!(report.mark(), Mark::Pass);
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).mark(), Mark::Checking);
    assert_eq!(report.line(false), "0 ms · in sync");
    assert_eq!(report.line(true), "0 ms · in sync · fastest");
    assert_eq!(report.rows[2].label(), "Up to date: block 1,792,004 · 1 min ago");
}

#[tokio::test(start_paused = true)]
async fn each_check_is_told_as_it_finishes() {
    let seen = Mutex::new(vec![]);
    let progress = |r: &Report| seen.lock().unwrap().push((r.verdict(), r.rows.iter().filter(|r| r.mark == Mark::Checking).count()));
    let report = run(&good(), Report::pending(Network::Stagenet, &node("node.example:38081")), 0, NOW, &progress).await;
    assert_eq!(*seen.lock().unwrap(), [("Checking…", 3), ("Checking…", 2), ("Checking…", 1)]);
    assert!(!report.checking());
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).line(false), "Checking…");
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).usable(), Usable::No);
}

#[tokio::test(start_paused = true)]
async fn a_node_that_never_answers_stops_at_answers() {
    let report = checked(&Fake { info: None, ..good() }, "node.example:38081").await;
    assert_eq!(
        rows(&report),
        [
            ("Answers", "didn't answer in 5 s", Mark::Fail),
            ("Right network", "", Mark::Skipped),
            ("Up to date", "", Mark::Skipped),
            ("Sends blocks to Fob", "", Mark::Skipped),
        ]
    );
    assert_eq!((report.verdict(), report.usable()), ("Doesn't answer", Usable::No));
    assert_eq!(report.line(false), "Didn't answer in 5 s");
    assert_eq!(report.rows[1].label(), "Right network: not checked");
    let refused = checked(&Fake { info: Some(Err(Failure::Refused)), ..good() }, "node.example:38081").await;
    assert_eq!(refused.line(false), "Refused to connect");
    let web = checked(&Fake { info: Some(Err(Failure::NotMonero)), ..good() }, "example.com").await;
    assert_eq!(web.line(false), "Not a Monero node");
}

#[tokio::test(start_paused = true)]
async fn a_mainnet_node_is_the_wrong_network_for_a_stagenet_wallet() {
    let mainnet = Fake { info: Some(Ok(Info { nettype: "mainnet".into(), height: 3_400_000, target_height: 0 })), ..good() };
    let report = checked(&mainnet, "xmr.example:18081").await;
    assert_eq!(rows(&report)[1], ("Right network", "mainnet, not stagenet", Mark::Fail));
    assert_eq!(rows(&report)[2..], [("Up to date", "", Mark::Skipped), ("Sends blocks to Fob", "", Mark::Skipped)]);
    assert_eq!((report.verdict(), report.usable()), ("Wrong network", Usable::No));
    assert_eq!(report.line(false), "Mainnet node: this wallet is stagenet");
    let unknown = checked(&Fake { info: Some(Ok(Info { nettype: String::new(), ..Info::default() })), ..good() }, "x:1").await;
    assert_eq!(unknown.line(false), "Unknown network: this wallet is stagenet");
}

#[tokio::test(start_paused = true)]
async fn a_lagging_node_can_be_used_anyway() {
    // Two days of blocks short of what it's catching up to.
    let lagging = Fake { info: Some(Ok(Info { nettype: "stagenet".into(), height: HEIGHT, target_height: HEIGHT + 1440 })), ..good() };
    let report = checked(&lagging, "node.example:38081").await;
    assert_eq!(rows(&report)[2..], [("Up to date", "2 days behind", Mark::Fail), ("Sends blocks to Fob", "31 blocks/s", Mark::Pass)]);
    assert_eq!(report.rows[3].second.as_deref(), Some("1.8 MB/s"));
    assert_eq!((report.verdict(), report.usable(), report.mark()), ("Not up to date", Usable::UseAnyway, Mark::Fail));
    assert_eq!(report.line(false), "0 ms · 2 days behind");
    // Behind what this wallet has already seen, though it thinks it's synced.
    let report = run(&good(), Report::pending(Network::Stagenet, &node("x:1")), HEIGHT + 30, NOW, &|_| {}).await;
    assert_eq!(rows(&report)[2], ("Up to date", "1 h behind", Mark::Fail));
    // A few blocks it hasn't heard of yet don't count.
    let report = run(&good(), Report::pending(Network::Stagenet, &node("x:1")), HEIGHT + 5, NOW, &|_| {}).await;
    assert_eq!(report.verdict(), "Working");
    // Stuck: no new block for three hours.
    let stuck = Fake { last_block: Some(Ok((HEIGHT - 1, NOW - 3 * 3600))), ..good() };
    assert_eq!(rows(&checked(&stuck, "x:1").await)[2], ("Up to date", "3 h behind", Mark::Fail));
    // Can't say its last block: not up to date, and not usable without it.
    let silent = Fake { last_block: None, ..good() };
    let report = checked(&silent, "x:1").await;
    assert_eq!(rows(&report)[2], ("Up to date", "didn't answer in 5 s", Mark::Fail));
    assert_eq!(report.usable(), Usable::UseAnyway);
}

#[tokio::test(start_paused = true)]
async fn a_node_that_sends_no_blocks_cant_be_used() {
    let slow = checked(&Fake { blocks: None, ..good() }, "x:1").await;
    assert_eq!(rows(&slow)[3], ("Sends blocks to Fob", "no blocks in 10 s", Mark::Fail));
    assert_eq!((slow.rows[3].second.as_deref(), slow.rows[3].label().as_str()), (None, "Sends blocks to Fob: no blocks in 10 s"));
    assert_eq!((slow.verdict(), slow.usable()), ("Doesn't send blocks", Usable::No));
    assert_eq!(slow.line(false), "0 ms · no blocks in 10 s");
    let garbled = checked(&Fake { blocks: Some(Err(Failure::Unreadable)), ..good() }, "x:1").await;
    assert_eq!(rows(&garbled)[3], ("Sends blocks to Fob", "sent blocks Fob can't read", Mark::Fail));
    let none = checked(&Fake { blocks: Some(Ok((0, 0))), ..good() }, "x:1").await;
    assert_eq!(rows(&none)[3], ("Sends blocks to Fob", "sent no blocks", Mark::Fail));
    // Behind and not sending: no "anyway".
    let both = Fake { blocks: None, last_block: Some(Ok((HEIGHT - 1, NOW - 4 * 86_400))), ..good() };
    let report = checked(&both, "x:1").await;
    assert_eq!((report.verdict(), report.usable()), ("Not up to date", Usable::No));
    let slow = checked(&Fake { blocks: Some(Ok((5, 40_000))), ..good() }, "x:1").await;
    assert_eq!((rows(&slow)[3].1, slow.rows[3].second.as_deref()), ("5.0 blocks/s", Some("40 kB/s")));
}

#[tokio::test(start_paused = true)]
async fn an_https_node_has_its_certificate_checked_first() {
    let valid = checked(&Fake { certificate: Some(Ok(())), ..good() }, "https://node.example:18089").await;
    assert_eq!(rows(&valid)[0], ("Certificate", "valid", Mark::Pass));
    assert_eq!((valid.rows.len(), valid.verdict()), (5, "Working"));
    // Marked to accept it: said, and the checks go on.
    let accepted = checked(&Fake { certificate: Some(Err(CertProblem::SelfSigned)), ..good() }, "https://192.168.1.20:38081").await;
    assert_eq!(rows(&accepted)[0], ("Certificate", "self-signed, accepted", Mark::Pass));
    assert_eq!((accepted.verdict(), accepted.address.as_str()), ("Working", "192.168.1.20:38081"));
    // Not marked: nothing else runs.
    let refused = checked(&Fake { info: Some(Err(Failure::Certificate(CertProblem::SelfSigned))), ..good() }, "https://192.168.1.20:38081").await;
    assert_eq!(
        rows(&refused),
        [
            ("Certificate", "self-signed, not accepted", Mark::Fail),
            ("Answers", "", Mark::Skipped),
            ("Right network", "", Mark::Skipped),
            ("Up to date", "", Mark::Skipped),
            ("Sends blocks to Fob", "", Mark::Skipped),
        ]
    );
    assert_eq!((refused.verdict(), refused.usable()), ("Certificate not accepted", Usable::No));
    assert_eq!(refused.line(false), "Certificate self-signed, not accepted");
    // No answer at all: the certificate was never seen.
    let silent = checked(&Fake { info: None, ..good() }, "https://node.example").await;
    assert_eq!(rows(&silent)[..2], [("Certificate", "", Mark::Skipped), ("Answers", "didn't answer in 5 s", Mark::Fail)]);
    assert_eq!(silent.address, "node.example");
}

#[tokio::test(start_paused = true)]
async fn the_fastest_of_the_nodes_that_pass_says_so() {
    let quick = checked(&Fake { blocks: Some(Ok((90, 1_000))), ..good() }, "a:1").await;
    let slow = checked(&good(), "b:1").await;
    let behind = checked(&Fake { info: Some(Ok(Info { nettype: "stagenet".into(), height: HEIGHT, target_height: HEIGHT + 900 })), ..good() }, "c:1").await;
    assert_eq!(lines(&[slow.clone(), quick.clone(), behind]), ["0 ms · in sync", "0 ms · in sync · fastest", "0 ms · 30 h behind"]);
    assert_eq!(lines(&[slow]), ["0 ms · in sync"], "fastest only among two or more");
}

#[test]
fn texts() {
    assert_eq!([ago(10), ago(75), ago(3599), ago(3600)], ["just now", "1 min ago", "59 min ago", "1 h ago"]);
    assert_eq!([behind(30), behind(720), behind(7200), behind(172_800)], ["1 min behind", "12 min behind", "2 h behind", "2 days behind"]);
    assert_eq!(address("https://user:secret@node.example:18089/"), "node.example:18089");
    assert_eq!(address("192.168.1.20:38081"), "192.168.1.20:38081");
    assert_eq!(address("http://[::1]:38081"), "[::1]:38081");
    assert_eq!(Failure::Status(401).words(ANSWER), "wants a login");
    assert_eq!(Failure::Onion.words(ANSWER), "needs Tor, not in Fob yet");
    assert_eq!(capitalized("éa"), "Éa");
}

#[test]
fn transport_errors_are_told_apart() {
    let refused = "error sending request: client error (Connect): tcp connect error: Connection refused (os error 111)";
    assert_eq!(Failure::from_text(refused), Failure::Refused);
    assert_eq!(Failure::from_text("error sending request: client error (Connect): dns error: failed to lookup address information"), Failure::NotFound);
    assert_eq!(Failure::from_text("node answered 401 Unauthorized"), Failure::Status(401));
    assert_eq!(Failure::from_text("node answered 503 Service Unavailable"), Failure::Status(503));
    assert_eq!(Failure::from_text("operation timed out"), Failure::TimedOut);
    assert_eq!(Failure::from_text("something else"), Failure::Other);
}

/// The sync card says why syncing failed as the checks would, never the
/// transport's debug text.
#[test]
fn sync_failures_are_told_in_words() {
    // Made as the sync loop makes them: the interface error's debug text.
    let node = |e: InterfaceError| FobError::Node(format!("{e:?}"));
    let words = |e: &FobError| sync_failure(e, None, false).0;
    let offline = node(InterfaceError::InterfaceError("error sending request: client error (Connect): dns error: failed to lookup address information: No address associated with hostname".into()));
    assert_eq!(words(&offline), "Address not found");
    let refused = node(InterfaceError::InterfaceError("error sending request: client error (Connect): tcp connect error: Connection refused (os error 111)".into()));
    assert_eq!(words(&refused), "Refused to connect");
    assert_eq!(words(&node(InterfaceError::InterfaceError("operation timed out".into()))), "Stopped answering");
    assert_eq!(words(&node(InterfaceError::InterfaceError("node answered 401 Unauthorized".into()))), "Wants a login");
    assert_eq!(words(&FobError::Node("scanning block 12: InvalidScannableBlock(\"x\")".into())), "Sent blocks Fob can't read");
    // The node answered, but not as the protocol says: the fetcher's and
    // monero-daemon-rpc's own words for it.
    for invalid in ["the node sent blocks out of order", "node's response was too large", "expected 100 blocks, got 3"] {
        assert_eq!(words(&node(InterfaceError::InvalidInterface(invalid.into()))), "Sent blocks Fob can't read", "{invalid}");
    }
    assert_eq!(words(&FobError::Node("rejected".into())), "Couldn't connect");
    // An older file's onion node, refused before anything connects.
    assert_eq!(words(&node(InterfaceError::InternalError(NO_ONION.into()))), "Needs Tor, not in Fob yet");
    // The phone's own failure: its words, and not the node's fault.
    assert_eq!(sync_failure(&FobError::Storage("disk full".into()), None, false), ("Couldn't read or save the wallet: disk full".into(), true));
    assert!(!sync_failure(&offline, None, false).1);
}

/// A failed round over https says what the Node checks say of the
/// certificate: refused as the node isn't marked, or never accepted.
#[test]
fn a_sync_refused_for_its_certificate_says_so() {
    let tls = FobError::Node(format!("{:?}", InterfaceError::InterfaceError("error sending request: client error (Connect): invalid peer certificate: Expired".into())));
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::Expired)), false).0, "Certificate expired, not accepted");
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::SelfSigned)), false).0, "Certificate self-signed, not accepted");
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::BadSignature)), true).0, "Certificate signature doesn't match, not accepted");
    // Accepted with the mark: the failure is something else.
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::SelfSigned)), true).0, "Couldn't connect");
    assert_eq!(sync_failure(&tls, Some(Ok(())), false).0, "Couldn't connect");
}

/// Over a real handshake: a node whose certificate the wallet's node
/// setting doesn't accept fails syncing in the Certificate's words.
#[tokio::test]
async fn syncing_against_a_refused_certificate_names_it() {
    let port = canned_node(true).await;
    let url = format!("https://127.0.0.1:{port}");
    let transport = Transport::new(&url, false).unwrap();
    let e = transport.post("get_info", b"{}".to_vec(), None).await.unwrap_err();
    let failed = FobError::Node(format!("{e:?}"));
    assert_eq!(sync_failure(&failed, transport.certificate(), false).0, "Certificate self-signed, not accepted");
}

#[test]
fn monerods_answers_are_read() {
    let info = br#"{"height":1792005,"target_height":0,"nettype":"stagenet","synchronized":true,"status":"OK"}"#;
    assert_eq!(parse_info(info), Ok(Info { nettype: "stagenet".into(), height: 1_792_005, target_height: 0 }));
    let old = br#"{"height":10,"target_height":500,"mainnet":false,"stagenet":true,"testnet":false}"#;
    assert_eq!(parse_info(old), Ok(Info { nettype: "stagenet".into(), height: 10, target_height: 500 }));
    assert_eq!(parse_info(b"<html>"), Err(Failure::NotMonero));
    assert_eq!(parse_info(b"{}"), Err(Failure::NotMonero));
    let header = br#"{"id":"0","jsonrpc":"2.0","result":{"block_header":{"height":1792004,"timestamp":1793499925,"hash":"ab"},"status":"OK"}}"#;
    assert_eq!(parse_last_block(header), Ok((1_792_004, 1_793_499_925)));
    assert_eq!(parse_last_block(br#"{"error":{"code":-32601}}"#), Err(Failure::NotMonero));
}

#[test]
fn onion_hosts_are_known_by_name() {
    let onion = "http://xmrnodexxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.onion:18081";
    for host in ["abc.onion", "ABC.ONION", "abc.onion.", "a.b.onion", "onion"] {
        assert!(is_onion(host), "{host}");
    }
    for host in ["onion.example", "example.com", "notonion", "1.2.3.4", "", "é.onio"] {
        assert!(!is_onion(host), "{host}");
    }
    assert_eq!(reachable(onion), Err(NO_ONION.into()));
    assert_eq!(reachable("xmrnode.onion:18081"), Err(NO_ONION.into()), "without a scheme too");
    assert_eq!(reachable("https://user:pw@Node.Onion./"), Err(NO_ONION.into()));
    assert_eq!(reachable("node.example:18081"), Ok(()));
}

/// Refused before anything connects or looks the name up: a resolver
/// would hear which onion service the wallet uses.
#[tokio::test]
async fn an_onion_node_is_refused_before_any_lookup() {
    let checked = check(Network::Stagenet, &node("xmrnode.onion:38081"), 0, NOW, &|_| {}).await;
    assert_eq!(rows(&checked)[0], ("Answers", "needs Tor, not in Fob yet", Mark::Fail));
    assert_eq!(checked.line(false), "Needs Tor, not in Fob yet");
    assert_eq!(on_network(Network::Stagenet, &node("http://xmrnode.onion:38081")).await, Err(NO_ONION.into()));
    let err = Transport::new("http://xmrnode.onion:38081", false).err().expect("refused");
    assert!(format!("{err:?}").contains(NO_ONION), "{err:?}");
}

#[test]
fn addresses_are_written_one_way() {
    assert_eq!(normalize(" node.example:18089/ "), "http://node.example:18089");
    assert_eq!(normalize("https://node.example:18089"), "https://node.example:18089");
}

/// A self-signed certificate for 127.0.0.1 and localhost (CA:TRUE, as
/// openssl makes them by default), valid until 2126, and its key.
const CERT: &[u8] = include_bytes!("self-signed.crt.der");
const KEY: &[u8] = include_bytes!("self-signed.key.der");
/// A certificate as monerod makes its own: version 1, no subject or
/// issuer, no extensions (which webpki won't read at all), and its key.
const MONEROD_CERT: &[u8] = include_bytes!("monerod-v1.crt.der");
const MONEROD_KEY: &[u8] = include_bytes!("monerod-v1.key.der");

/// How [`canned_node`] answers: plain HTTP, or TLS with a certificate.
#[derive(Clone, Copy)]
pub(crate) enum Canned {
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
pub(crate) async fn canned_node(tls: bool) -> u16 {
    canned(if tls { Canned::Tls { monerod: false, tls12: false } } else { Canned::Http }).await
}

/// [`canned_node`], answering as `how` says.
pub(crate) async fn canned(how: Canned) -> u16 {
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

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

#[tokio::test]
async fn a_canned_node_is_checked_over_http() {
    let port = canned_node(false).await;
    let report = check(Network::Stagenet, &node(&format!("127.0.0.1:{port}")), 0, now(), &|_| {}).await;
    let rows = rows(&report);
    assert_eq!(rows.len(), 4, "no certificate over http");
    assert_eq!((rows[0].0, rows[0].2), ("Answers", Mark::Pass));
    assert_eq!(rows[1..3], [("Right network", "stagenet", Mark::Pass), ("Up to date", "block 1,792,004 · just now", Mark::Pass)]);
    assert_eq!(rows[3], ("Sends blocks to Fob", "sent blocks Fob can't read", Mark::Fail));
    assert_eq!(report.address, format!("127.0.0.1:{port}"));
}

#[tokio::test]
async fn a_closed_port_refuses_to_connect() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let report = check(Network::Stagenet, &node(&format!("http://127.0.0.1:{port}")), 0, now(), &|_| {}).await;
    assert_eq!(rows(&report)[0], ("Answers", "refused to connect", Mark::Fail));
}

/// The usual check refuses a self-signed certificate and says so; a node
/// marked to accept it gets through, and says what it accepted.
#[tokio::test]
async fn a_self_signed_certificate_passes_only_when_accepted() {
    let port = canned_node(true).await;
    let url = format!("https://127.0.0.1:{port}");
    let refused = check(Network::Stagenet, &node(&url), 0, now(), &|_| {}).await;
    assert_eq!(rows(&refused)[..2], [("Certificate", "self-signed, not accepted", Mark::Fail), ("Answers", "", Mark::Skipped)]);
    assert_eq!(refused.verdict(), "Certificate not accepted");
    let accepted = check(Network::Stagenet, &Setting { url: url.clone(), self_signed: true }, 0, now(), &|_| {}).await;
    assert_eq!(rows(&accepted)[..3], [("Certificate", "self-signed, accepted", Mark::Pass), ("Answers", rows(&accepted)[1].1, Mark::Pass), ("Right network", "stagenet", Mark::Pass)]);
    // Switching to it checks the same way.
    assert_eq!(on_network(Network::Stagenet, &node(&url)).await, Err("self-signed, not accepted".into()));
    assert_eq!(on_network(Network::Stagenet, &Setting { url, self_signed: true }).await, Ok(()));
}

/// monerod's own certificate (version 1, as `--rpc-ssl autodetect` makes
/// it) is refused as self-signed, and works once accepted, over TLS 1.3
/// and 1.2: the handshake's signature is checked against its key.
#[tokio::test]
async fn monerods_own_certificate_works_once_accepted() {
    for tls12 in [false, true] {
        let port = canned(Canned::Tls { monerod: true, tls12 }).await;
        let url = format!("https://127.0.0.1:{port}");
        let refused = check(Network::Stagenet, &node(&url), 0, now(), &|_| {}).await;
        assert_eq!(rows(&refused)[0], ("Certificate", "self-signed, not accepted", Mark::Fail), "TLS 1.2: {tls12}");
        let accepted = check(Network::Stagenet, &Setting { url: url.clone(), self_signed: true }, 0, now(), &|_| {}).await;
        assert_eq!(rows(&accepted)[0], ("Certificate", "self-signed, accepted", Mark::Pass), "TLS 1.2: {tls12}");
        assert_eq!(rows(&accepted)[2], ("Right network", "stagenet", Mark::Pass), "TLS 1.2: {tls12}");
        assert_eq!(on_network(Network::Stagenet, &Setting { url, self_signed: true }).await, Ok(()));
    }
}

/// Accepting a node's own certificate still needs the handshake signed by
/// its key: someone answering with a certificate whose key they don't hold
/// is refused, over TLS 1.3 and 1.2, with an EC or an RSA key, and the
/// Certificate row says so.
#[tokio::test]
async fn a_handshake_not_signed_by_the_certificates_key_is_refused_even_when_accepted() {
    for monerod in [false, true] {
        for tls12 in [false, true] {
            let port = canned(Canned::WrongKey { monerod, tls12 }).await;
            let url = format!("https://127.0.0.1:{port}");
            let marked = check(Network::Stagenet, &Setting { url: url.clone(), self_signed: true }, 0, now(), &|_| {}).await;
            let case = format!("monerod: {monerod}, TLS 1.2: {tls12}");
            assert_eq!(rows(&marked)[..2], [("Certificate", "signature doesn't match, not accepted", Mark::Fail), ("Answers", "", Mark::Skipped)], "{case}");
            assert_eq!(marked.line(false), "Certificate signature doesn't match, not accepted", "{case}");
            assert_eq!(on_network(Network::Stagenet, &Setting { url, self_signed: true }).await, Err("signature doesn't match, not accepted".into()), "{case}");
        }
    }
}

/// A node that sends Fob elsewhere isn't followed: the other host would get
/// this node's certificate rule (or an onion name looked up in the clear).
#[tokio::test]
async fn a_node_that_redirects_is_not_followed() {
    let good = canned_node(false).await;
    let port = canned(Canned::Redirect(good)).await;
    let report = check(Network::Stagenet, &node(&format!("127.0.0.1:{port}")), 0, now(), &|_| {}).await;
    assert_eq!(rows(&report)[0], ("Answers", "answered 307", Mark::Fail));
    assert_eq!(report.verdict(), "Doesn't answer");
}

/// Only a problem a node's own certificate can have is accepted with the
/// mark; one that isn't says so in its own words.
#[test]
fn only_a_nodes_own_certificate_problems_can_be_accepted() {
    use rustls::CertificateError as C;
    #[derive(Debug)]
    struct Webpki(&'static str);
    impl std::fmt::Display for Webpki {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for Webpki {}
    let webpki = |name| rustls::Error::InvalidCertificate(C::Other(rustls::OtherError(Arc::new(Webpki(name)))));
    let of = |e: C| CertProblem::of(&rustls::Error::InvalidCertificate(e));
    assert_eq!(of(C::UnknownIssuer), CertProblem::SelfSigned);
    assert_eq!(CertProblem::of(&webpki("CaUsedAsEndEntity")), CertProblem::SelfSigned);
    assert_eq!(CertProblem::of(&webpki("UnsupportedCertVersion")), CertProblem::SelfSigned);
    assert_eq!((of(C::Expired), of(C::NotValidYet), of(C::NotValidForName)), (CertProblem::Expired, CertProblem::NotYetValid, CertProblem::OtherName));
    for broken in [C::BadSignature, C::Revoked, C::BadEncoding, C::UnhandledCriticalExtension, C::InvalidPurpose] {
        assert_eq!(of(broken.clone()), CertProblem::Invalid, "{broken:?}");
    }
    assert_eq!(CertProblem::of(&webpki("EndEntityUsedAsCa")), CertProblem::Invalid);
    assert_eq!(CertProblem::of(&rustls::Error::General("x".into())), CertProblem::Invalid);
    for (problem, acceptable) in [(CertProblem::SelfSigned, true), (CertProblem::Expired, true), (CertProblem::OtherName, true), (CertProblem::BadSignature, false), (CertProblem::Invalid, false)] {
        assert_eq!(problem.acceptable(), acceptable, "{problem:?}");
        let refused = Failure::refused(Some(Err(problem)), true);
        assert_eq!(refused, (!acceptable).then_some(Failure::Certificate(problem)), "marked: {problem:?}");
        assert_eq!(Failure::refused(Some(Err(problem)), false), Some(Failure::Certificate(problem)), "not marked: {problem:?}");
    }
    assert_eq!(Failure::Certificate(CertProblem::Invalid).words(ANSWER), "not valid, not accepted");
    assert_eq!(Failure::refused(Some(Ok(())), false), None);
}

/// A certificate's key is read without webpki, from any version.
#[test]
fn a_certificates_key_is_read_from_any_version() {
    let (_, algorithm, key) = crate::transport::public_key(MONEROD_CERT).unwrap();
    // rsaEncryption, NULL.
    assert_eq!(algorithm, [0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00]);
    assert_eq!(key[0], 0x30, "an RSA key is a SEQUENCE");
    assert!(crate::transport::public_key(CERT).is_some(), "version 3 too");
    assert_eq!(crate::transport::public_key(&MONEROD_CERT[..40]), None, "cut short");
    assert_eq!(crate::transport::public_key(b"not a certificate"), None);
    // Hostile lengths and shapes: none read, none panic.
    let hostile: [&[u8]; 9] = [
        &[0x30, 0x84, 0xff, 0xff, 0xff, 0xff],
        &[0x30, 0x80, 0x00, 0x00],
        &[0x30, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00],
        &[0x30, 0x81],
        &[0x30],
        &[],
        // A tbs whose [0] version says more than there is.
        &[0x30, 0x06, 0x30, 0x04, 0xa0, 0x7f, 0x02, 0x01],
        // A whole SPKI whose key has unused bits.
        &[0x30, 0x19, 0x30, 0x17, 0x02, 0x01, 0x01, 0x30, 0x00, 0x30, 0x00, 0x30, 0x00, 0x30, 0x00, 0x30, 0x0a, 0x30, 0x03, 0x06, 0x01, 0x00, 0x03, 0x03, 0x01, 0xaa, 0xbb],
        // A long form padded past four bytes.
        &[0x30, 0x88, 0, 0, 0, 0, 0, 0, 0, 1, 0],
    ];
    for bytes in hostile {
        assert_eq!(crate::transport::public_key(bytes), None, "{bytes:02x?}");
    }
    // The same SPKI with no unused bits reads.
    let whole = [0x30, 0x19, 0x30, 0x17, 0x02, 0x01, 0x01, 0x30, 0x00, 0x30, 0x00, 0x30, 0x00, 0x30, 0x00, 0x30, 0x0a, 0x30, 0x03, 0x06, 0x01, 0x00, 0x03, 0x03, 0x00, 0xaa, 0xbb];
    assert_eq!(crate::transport::public_key(&whole).map(|(_, a, k)| (a.to_vec(), k.to_vec())), Some((vec![0x06, 0x01, 0x00], vec![0xaa, 0xbb])));
    // Every cut of a real certificate fails cleanly.
    for n in 0..MONEROD_CERT.len() {
        let _ = crate::transport::public_key(&MONEROD_CERT[..n]);
    }
}

#[tokio::test]
#[ignore = "needs public nodes"]
async fn checks_tell_fast_from_wrong_over_http() {
    let stagenet = check(Network::Stagenet, &node(Network::Stagenet.default_nodes()[0]), 0, now(), &|_| {}).await;
    println!("{stagenet:?}");
    assert_eq!(stagenet.verdict(), "Working");
    let mainnet_node = check(Network::Stagenet, &node(Network::Mainnet.default_nodes()[0]), 0, now(), &|_| {}).await;
    assert_eq!(mainnet_node.verdict(), "Wrong network");
}
