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
            ("Sends blocks to Fob", "31 blocks/s · 1.8 MB/s", Mark::Pass),
        ]
    );
    assert_eq!((report.verdict(), report.usable(), report.address.as_str()), ("Working", Usable::Use, "node.example:38081"));
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
    assert_eq!(rows(&report)[2..], [("Up to date", "2 days behind", Mark::Fail), ("Sends blocks to Fob", "31 blocks/s · 1.8 MB/s", Mark::Pass)]);
    assert_eq!((report.verdict(), report.usable()), ("Not up to date", Usable::UseAnyway));
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
    assert_eq!(rows(&slow)[3].1, "5.0 blocks/s · 40 kB/s");
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
    let offline = FobError::Node(r#"InterfaceError("error sending request: client error (Connect): dns error: failed to lookup address information: No address associated with hostname")"#.into());
    assert_eq!(sync_words(&offline), "Address not found");
    let refused = FobError::Node("InterfaceError(\"error sending request: client error (Connect): tcp connect error: Connection refused (os error 111)\")".into());
    assert_eq!(sync_words(&refused), "Refused to connect");
    assert_eq!(sync_words(&FobError::Node("InterfaceError(\"operation timed out\")".into())), "Stopped answering");
    assert_eq!(sync_words(&FobError::Node("InterfaceError(\"node answered 401 Unauthorized\")".into())), "Wants a login");
    assert_eq!(sync_words(&FobError::Node("scanning block 12: InvalidScannableBlock(\"x\")".into())), "Sent blocks Fob can't read");
    assert_eq!(sync_words(&FobError::Node("InvalidNode(\"bad\")".into())), "Sent blocks Fob can't read");
    assert_eq!(sync_words(&FobError::Node("rejected".into())), "Couldn't connect");
    assert_eq!(sync_words(&FobError::Storage("disk full".into())), "Couldn't read or save the wallet: disk full");
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

/// A stagenet node on 127.0.0.1 that answers get_info and the last block
/// header, and sends blocks that aren't blocks; over TLS with [`CERT`]
/// when `tls`. Gives its port.
pub(crate) async fn canned_node(tls: bool) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = tls.then(|| {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CERT.to_vec().into()], rustls::pki_types::PrivatePkcs8KeyDer::from(KEY.to_vec()).into())
            .unwrap();
        tokio_rustls::TlsAcceptor::from(Arc::new(config))
    });
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                match acceptor {
                    // A client that refuses the certificate hangs up here.
                    Some(acceptor) => {
                        if let Ok(stream) = acceptor.accept(stream).await {
                            answer(stream).await;
                        }
                    }
                    None => answer(stream).await,
                }
            });
        }
    });
    port
}

/// Answers one request, then closes.
async fn answer(mut stream: impl AsyncRead + AsyncWrite + Unpin) {
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
    let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
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

#[tokio::test]
#[ignore = "needs public nodes"]
async fn checks_tell_fast_from_wrong_over_http() {
    let stagenet = check(Network::Stagenet, &node(Network::Stagenet.default_nodes()[0]), 0, now(), &|_| {}).await;
    println!("{stagenet:?}");
    assert_eq!(stagenet.verdict(), "Working");
    let mainnet_node = check(Network::Stagenet, &node(Network::Mainnet.default_nodes()[0]), 0, now(), &|_| {}).await;
    assert_eq!(mainnet_node.verdict(), "Wrong network");
}
