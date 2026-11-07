use std::sync::{Arc, Mutex};

use super::canned::*;
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
        match self.last_block {
            Some(last) => last,
            None => std::future::pending().await,
        }
    }

    async fn blocks(&self, start: u64, end: u64) -> Result<(u64, u64), Failure> {
        assert!(start >= 1 && start <= end && end - start < SAMPLE, "{start}..={end}");
        match self.blocks {
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

/// A row as (check, outcome, mark), its speed rounded as the clock's a moment over a second.
fn rows(report: &Report) -> Vec<(Step, Outcome, Mark)> {
    report
        .rows
        .iter()
        .map(|r| {
            let outcome = match r.outcome {
                Outcome::Speed { blocks_per_second, bytes_per_second } => Outcome::Speed { blocks_per_second: (blocks_per_second * 10.0).round() / 10.0, bytes_per_second: (bytes_per_second / 1e4).round() * 1e4 },
                ref other => other.clone(),
            };
            (r.step, outcome, r.mark)
        })
        .collect()
}

fn failed(failure: Failure) -> Outcome {
    Outcome::Failed { failure, limit: 5 }
}

fn block(age: u64) -> Outcome {
    Outcome::Block { height: 1_792_004, age }
}

fn network() -> Outcome {
    Outcome::Network("stagenet".into())
}

fn speed(blocks_per_second: f64, bytes_per_second: f64) -> Outcome {
    Outcome::Speed { blocks_per_second, bytes_per_second }
}

const NOT_ACCEPTED: Failure = Failure::Certificate(CertProblem::SelfSigned);

#[tokio::test(start_paused = true)]
async fn a_good_node_passes_every_check() {
    let report = checked(&good(), "node.example:38081").await;
    assert_eq!(
        rows(&report),
        [
            (Step::Answers, Outcome::Latency(0), Mark::Pass),
            (Step::Network, network(), Mark::Pass),
            (Step::UpToDate, block(75), Mark::Pass),
            (Step::Blocks, speed(31.0, 1_800_000.0), Mark::Pass),
        ]
    );
    assert_eq!((report.verdict(), report.usable(), report.address.as_str()), (Verdict::Working, Usable::Use, "node.example:38081"));
    assert_eq!(report.mark(), Mark::Pass);
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).mark(), Mark::Checking);
    assert_eq!(report.line(false), Line::InSync { ms: 0, fastest: false });
    assert_eq!(report.line(true), Line::InSync { ms: 0, fastest: true });
}

#[tokio::test(start_paused = true)]
async fn each_check_is_told_as_it_finishes() {
    let seen = Mutex::new(vec![]);
    let progress = |r: &Report| seen.lock().unwrap().push((r.verdict(), r.rows.iter().filter(|r| r.mark == Mark::Checking).count()));
    let report = run(&good(), Report::pending(Network::Stagenet, &node("node.example:38081")), 0, NOW, &progress).await;
    assert_eq!(*seen.lock().unwrap(), [(Verdict::Checking, 3), (Verdict::Checking, 2), (Verdict::Checking, 1)]);
    assert!(!report.checking());
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).line(false), Line::Checking);
    assert_eq!(Report::pending(Network::Stagenet, &node("x:1")).usable(), Usable::No);
}

#[tokio::test(start_paused = true)]
async fn a_node_that_never_answers_stops_at_answers() {
    let report = checked(&Fake { info: None, ..good() }, "node.example:38081").await;
    assert_eq!(
        rows(&report),
        [
            (Step::Answers, failed(Failure::TimedOut), Mark::Fail),
            (Step::Network, Outcome::Skipped, Mark::Skipped),
            (Step::UpToDate, Outcome::Skipped, Mark::Skipped),
            (Step::Blocks, Outcome::Skipped, Mark::Skipped),
        ]
    );
    assert_eq!((report.verdict(), report.usable()), (Verdict::Failed(Step::Answers), Usable::No));
    assert_eq!(report.line(false), Line::NoAnswer(failed(Failure::TimedOut)));
    let refused = checked(&Fake { info: Some(Err(Failure::Refused)), ..good() }, "node.example:38081").await;
    assert_eq!(refused.line(false), Line::NoAnswer(failed(Failure::Refused)));
    let web = checked(&Fake { info: Some(Err(Failure::NotMonero)), ..good() }, "example.com").await;
    assert_eq!(web.line(false), Line::NoAnswer(failed(Failure::NotMonero)));
}

#[tokio::test(start_paused = true)]
async fn a_mainnet_node_is_the_wrong_network_for_a_stagenet_wallet() {
    let mainnet = Fake { info: Some(Ok(Info { nettype: "mainnet".into(), height: 3_400_000, target_height: 0 })), ..good() };
    let report = checked(&mainnet, "xmr.example:18081").await;
    assert_eq!(rows(&report)[1], (Step::Network, Outcome::WrongNetwork { theirs: "mainnet".into(), ours: "stagenet".into() }, Mark::Fail));
    assert_eq!(rows(&report)[2..], [(Step::UpToDate, Outcome::Skipped, Mark::Skipped), (Step::Blocks, Outcome::Skipped, Mark::Skipped)]);
    assert_eq!((report.verdict(), report.usable()), (Verdict::Failed(Step::Network), Usable::No));
    assert_eq!(report.line(false), Line::OtherNetwork { theirs: "mainnet".into(), ours: "stagenet".into() });
    let unknown = checked(&Fake { info: Some(Ok(Info { nettype: String::new(), ..Info::default() })), ..good() }, "x:1").await;
    assert_eq!(unknown.line(false), Line::OtherNetwork { theirs: String::new(), ours: "stagenet".into() });
}

#[tokio::test(start_paused = true)]
async fn a_lagging_node_can_be_used_anyway() {
    // Two days of blocks short of what it's catching up to.
    let lagging = Fake { info: Some(Ok(Info { nettype: "stagenet".into(), height: HEIGHT, target_height: HEIGHT + 1440 })), ..good() };
    let report = checked(&lagging, "node.example:38081").await;
    let two_days = Outcome::Behind { seconds: 172_800 };
    assert_eq!(rows(&report)[2..], [(Step::UpToDate, two_days.clone(), Mark::Fail), (Step::Blocks, speed(31.0, 1_800_000.0), Mark::Pass)]);
    assert_eq!((report.verdict(), report.usable(), report.mark()), (Verdict::Failed(Step::UpToDate), Usable::UseAnyway, Mark::Fail));
    assert_eq!(report.line(false), Line::Poor { ms: 0, outcome: two_days });
    // Behind what this wallet has already seen, though it thinks it's synced.
    let report = run(&good(), Report::pending(Network::Stagenet, &node("x:1")), HEIGHT + 30, NOW, &|_| {}).await;
    assert_eq!(rows(&report)[2], (Step::UpToDate, Outcome::Behind { seconds: 3600 }, Mark::Fail));
    // A few blocks it hasn't heard of yet don't count.
    let report = run(&good(), Report::pending(Network::Stagenet, &node("x:1")), HEIGHT + 5, NOW, &|_| {}).await;
    assert_eq!(report.verdict(), Verdict::Working);
    // Stuck: no new block for three hours.
    let stuck = Fake { last_block: Some(Ok((HEIGHT - 1, NOW - 3 * 3600))), ..good() };
    assert_eq!(rows(&checked(&stuck, "x:1").await)[2], (Step::UpToDate, Outcome::Behind { seconds: 3 * 3600 }, Mark::Fail));
    // Can't say its last block: not up to date, and not usable without it.
    let silent = Fake { last_block: None, ..good() };
    let report = checked(&silent, "x:1").await;
    assert_eq!(rows(&report)[2], (Step::UpToDate, failed(Failure::TimedOut), Mark::Fail));
    assert_eq!(report.usable(), Usable::UseAnyway);
}

#[tokio::test(start_paused = true)]
async fn a_node_that_sends_no_blocks_cant_be_used() {
    let slow = checked(&Fake { blocks: None, ..good() }, "x:1").await;
    let within = Outcome::NoBlocksWithin { seconds: 10 };
    assert_eq!(rows(&slow)[3], (Step::Blocks, within.clone(), Mark::Fail));
    assert_eq!((slow.verdict(), slow.usable()), (Verdict::Failed(Step::Blocks), Usable::No));
    assert_eq!(slow.line(false), Line::Poor { ms: 0, outcome: within });
    let garbled = checked(&Fake { blocks: Some(Err(Failure::Unreadable)), ..good() }, "x:1").await;
    assert_eq!(rows(&garbled)[3], (Step::Blocks, Outcome::Failed { failure: Failure::Unreadable, limit: 10 }, Mark::Fail));
    let none = checked(&Fake { blocks: Some(Ok((0, 0))), ..good() }, "x:1").await;
    assert_eq!(rows(&none)[3], (Step::Blocks, Outcome::NoBlocks, Mark::Fail));
    // Behind and not sending: no "anyway".
    let both = Fake { blocks: None, last_block: Some(Ok((HEIGHT - 1, NOW - 4 * 86_400))), ..good() };
    let report = checked(&both, "x:1").await;
    assert_eq!((report.verdict(), report.usable()), (Verdict::Failed(Step::UpToDate), Usable::No));
    let slow = checked(&Fake { blocks: Some(Ok((5, 40_000))), ..good() }, "x:1").await;
    assert_eq!(rows(&slow)[3].1, speed(5.0, 40_000.0));
}

#[tokio::test(start_paused = true)]
async fn an_https_node_has_its_certificate_checked_first() {
    let valid = checked(&Fake { certificate: Some(Ok(())), ..good() }, "https://node.example:18089").await;
    assert_eq!(rows(&valid)[0], (Step::Certificate, Outcome::Certificate(None), Mark::Pass));
    assert_eq!((valid.rows.len(), valid.verdict()), (5, Verdict::Working));
    // Marked to accept it: said, and the checks go on.
    let accepted = checked(&Fake { certificate: Some(Err(CertProblem::SelfSigned)), ..good() }, "https://192.168.1.20:38081").await;
    assert_eq!(rows(&accepted)[0], (Step::Certificate, Outcome::Certificate(Some(CertProblem::SelfSigned)), Mark::Pass));
    assert_eq!((accepted.verdict(), accepted.address.as_str()), (Verdict::Working, "192.168.1.20:38081"));
    // Not marked: nothing else runs.
    let refused = checked(&Fake { info: Some(Err(NOT_ACCEPTED)), ..good() }, "https://192.168.1.20:38081").await;
    assert_eq!(
        rows(&refused),
        [
            (Step::Certificate, failed(NOT_ACCEPTED), Mark::Fail),
            (Step::Answers, Outcome::Skipped, Mark::Skipped),
            (Step::Network, Outcome::Skipped, Mark::Skipped),
            (Step::UpToDate, Outcome::Skipped, Mark::Skipped),
            (Step::Blocks, Outcome::Skipped, Mark::Skipped),
        ]
    );
    assert_eq!((refused.verdict(), refused.usable()), (Verdict::Failed(Step::Certificate), Usable::No));
    assert_eq!(refused.line(false), Line::Certificate(failed(NOT_ACCEPTED)));
    // No answer at all: the certificate was never seen.
    let silent = checked(&Fake { info: None, ..good() }, "https://node.example").await;
    assert_eq!(rows(&silent)[..2], [(Step::Certificate, Outcome::Skipped, Mark::Skipped), (Step::Answers, failed(Failure::TimedOut), Mark::Fail)]);
    assert_eq!(silent.address, "node.example");
}

#[tokio::test(start_paused = true)]
async fn the_fastest_of_the_nodes_that_pass_says_so() {
    let quick = checked(&Fake { blocks: Some(Ok((90, 1_000))), ..good() }, "a:1").await;
    let slow = checked(&good(), "b:1").await;
    let behind = checked(&Fake { info: Some(Ok(Info { nettype: "stagenet".into(), height: HEIGHT, target_height: HEIGHT + 900 })), ..good() }, "c:1").await;
    let in_sync = |fastest| Line::InSync { ms: 0, fastest };
    assert_eq!(lines(&[slow.clone(), quick.clone(), behind]), [in_sync(false), in_sync(true), Line::Poor { ms: 0, outcome: Outcome::Behind { seconds: 108_000 } }]);
    assert_eq!(lines(&[slow]), [in_sync(false)], "fastest only among two or more");
}

#[test]
fn addresses_show_host_and_port_alone() {
    assert_eq!(address("https://user:secret@node.example:18089/"), "node.example:18089");
    assert_eq!(address("192.168.1.20:38081"), "192.168.1.20:38081");
    assert_eq!(address("http://[::1]:38081"), "[::1]:38081");
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

/// Syncing's failures are told apart as the checks would, never by showing
/// the transport's debug text.
#[test]
fn sync_failures_are_told_apart() {
    // Made as the sync loop makes them: the interface error's debug text.
    let node = |e: InterfaceError| format!("{e:?}");
    let why = |e: &str| sync_failure(e, None, false);
    let offline = node(InterfaceError::InterfaceError("error sending request: client error (Connect): dns error: failed to lookup address information: No address associated with hostname".into()));
    assert_eq!(why(&offline), Failure::NotFound);
    let refused = node(InterfaceError::InterfaceError("error sending request: client error (Connect): tcp connect error: Connection refused (os error 111)".into()));
    assert_eq!(why(&refused), Failure::Refused);
    assert_eq!(why(&node(InterfaceError::InterfaceError("operation timed out".into()))), Failure::TimedOut);
    assert_eq!(why(&node(InterfaceError::InterfaceError("node answered 401 Unauthorized".into()))), Failure::Status(401));
    assert_eq!(why("scanning block 12: InvalidScannableBlock(\"x\")"), Failure::Unreadable);
    // The node answered, but not as the protocol says: the fetcher's and
    // monero-daemon-rpc's own words for it.
    for invalid in ["the node sent blocks out of order", "node's response was too large", "expected 100 blocks, got 3"] {
        assert_eq!(why(&node(InterfaceError::InvalidInterface(invalid.into()))), Failure::Unreadable, "{invalid}");
    }
    assert_eq!(why("rejected"), Failure::Other);
    // An older file's onion node, refused before anything connects.
    assert_eq!(why(&node(InterfaceError::InternalError(ONION_REFUSED.into()))), Failure::Onion);
}

/// A failed round over https says what the Node checks say of the
/// certificate: refused as the node isn't marked, or never accepted.
#[test]
fn a_sync_refused_for_its_certificate_says_so() {
    let tls = format!("{:?}", InterfaceError::InterfaceError("error sending request: client error (Connect): invalid peer certificate: Expired".into()));
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::Expired)), false), Failure::Certificate(CertProblem::Expired));
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::SelfSigned)), false), NOT_ACCEPTED);
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::BadSignature)), true), Failure::Certificate(CertProblem::BadSignature));
    // Accepted with the mark: the failure is something else.
    assert_eq!(sync_failure(&tls, Some(Err(CertProblem::SelfSigned)), true), Failure::Other);
    assert_eq!(sync_failure(&tls, Some(Ok(())), false), Failure::Other);
}

/// Over a real handshake: a node whose certificate the wallet's node
/// setting doesn't accept fails syncing in the Certificate's words.
#[tokio::test]
async fn syncing_against_a_refused_certificate_names_it() {
    let port = canned_node(true).await;
    let url = format!("https://127.0.0.1:{port}");
    let transport = Transport::new(&url, false).unwrap();
    let e = transport.post("get_info", b"{}".to_vec(), None).await.unwrap_err();
    let failed = format!("{e:?}");
    assert_eq!(sync_failure(&failed, transport.certificate(), false), NOT_ACCEPTED);
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
    assert_eq!(reachable(onion), Err(EndpointProblem::Onion));
    assert_eq!(reachable("xmrnode.onion:18081"), Err(EndpointProblem::Onion), "without a scheme too");
    assert_eq!(reachable("https://user:pw@Node.Onion./"), Err(EndpointProblem::Onion));
    assert_eq!(reachable("node.example:18081"), Ok(()));
}

/// Refused before anything connects or looks the name up: a resolver
/// would hear which onion service the wallet uses.
#[tokio::test]
async fn an_onion_node_is_refused_before_any_lookup() {
    let checked = check(Network::Stagenet, &node("xmrnode.onion:38081"), 0, NOW, &|_| {}).await;
    assert_eq!(rows(&checked)[0], (Step::Answers, failed(Failure::Onion), Mark::Fail));
    assert_eq!(checked.line(false), Line::NoAnswer(failed(Failure::Onion)));
    assert_eq!(on_network(Network::Stagenet, &node("http://xmrnode.onion:38081")).await, Err(SwitchRefused::Endpoint(EndpointProblem::Onion)));
    let err = Transport::new("http://xmrnode.onion:38081", false).err().expect("refused");
    assert!(format!("{err:?}").contains(ONION_REFUSED), "{err:?}");
}

#[test]
fn addresses_are_written_one_way() {
    assert_eq!(normalize(" node.example:18089/ "), "http://node.example:18089");
    assert_eq!(normalize("https://node.example:18089"), "https://node.example:18089");
}


#[tokio::test]
async fn a_canned_node_is_checked_over_http() {
    let port = canned_node(false).await;
    let report = check(Network::Stagenet, &node(&format!("127.0.0.1:{port}")), 0, now(), &|_| {}).await;
    let rows = rows(&report);
    assert_eq!(rows.len(), 4, "no certificate over http");
    assert_eq!((rows[0].0, rows[0].2), (Step::Answers, Mark::Pass));
    assert_eq!((&rows[1], &rows[2].0, &rows[2].2), (&(Step::Network, network(), Mark::Pass), &Step::UpToDate, &Mark::Pass));
    assert!(matches!(rows[2].1, Outcome::Block { height: 1_792_004, age: 0..60 }), "{:?}", rows[2].1);
    assert_eq!(rows[3], (Step::Blocks, Outcome::Failed { failure: Failure::Unreadable, limit: 10 }, Mark::Fail));
    assert_eq!(report.address, format!("127.0.0.1:{port}"));
}

#[tokio::test]
async fn a_closed_port_refuses_to_connect() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let report = check(Network::Stagenet, &node(&format!("http://127.0.0.1:{port}")), 0, now(), &|_| {}).await;
    assert_eq!(rows(&report)[0], (Step::Answers, failed(Failure::Refused), Mark::Fail));
}

/// The usual check refuses a self-signed certificate and says so; a node
/// marked to accept it gets through, and says what it accepted.
#[tokio::test]
async fn a_self_signed_certificate_passes_only_when_accepted() {
    let port = canned_node(true).await;
    let url = format!("https://127.0.0.1:{port}");
    let refused = check(Network::Stagenet, &node(&url), 0, now(), &|_| {}).await;
    assert_eq!(rows(&refused)[..2], [(Step::Certificate, failed(NOT_ACCEPTED), Mark::Fail), (Step::Answers, Outcome::Skipped, Mark::Skipped)]);
    assert_eq!(refused.verdict(), Verdict::Failed(Step::Certificate));
    let accepted = check(Network::Stagenet, &Setting { url: url.clone(), self_signed: true }, 0, now(), &|_| {}).await;
    assert_eq!(rows(&accepted)[..3], [(Step::Certificate, Outcome::Certificate(Some(CertProblem::SelfSigned)), Mark::Pass), (Step::Answers, rows(&accepted)[1].1.clone(), Mark::Pass), (Step::Network, network(), Mark::Pass)]);
    // Switching to it checks the same way.
    assert_eq!(on_network(Network::Stagenet, &node(&url)).await, Err(SwitchRefused::Failed(NOT_ACCEPTED)));
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
        assert_eq!(rows(&refused)[0], (Step::Certificate, failed(NOT_ACCEPTED), Mark::Fail), "TLS 1.2: {tls12}");
        let accepted = check(Network::Stagenet, &Setting { url: url.clone(), self_signed: true }, 0, now(), &|_| {}).await;
        assert_eq!(rows(&accepted)[0], (Step::Certificate, Outcome::Certificate(Some(CertProblem::SelfSigned)), Mark::Pass), "TLS 1.2: {tls12}");
        assert_eq!(rows(&accepted)[2], (Step::Network, network(), Mark::Pass), "TLS 1.2: {tls12}");
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
            let bad = Failure::Certificate(CertProblem::BadSignature);
            assert_eq!(rows(&marked)[..2], [(Step::Certificate, failed(bad), Mark::Fail), (Step::Answers, Outcome::Skipped, Mark::Skipped)], "{case}");
            assert_eq!(marked.line(false), Line::Certificate(failed(bad)), "{case}");
            assert_eq!(on_network(Network::Stagenet, &Setting { url, self_signed: true }).await, Err(SwitchRefused::Failed(bad)), "{case}");
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
    assert_eq!(rows(&report)[0], (Step::Answers, failed(Failure::Status(307)), Mark::Fail));
    assert_eq!(report.verdict(), Verdict::Failed(Step::Answers));
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
    for (problem, acceptable) in [(CertProblem::SelfSigned, true), (CertProblem::Expired, true), (CertProblem::OtherName, false), (CertProblem::Changed, false), (CertProblem::BadSignature, false), (CertProblem::Invalid, false)] {
        assert_eq!(problem.acceptable(), acceptable, "{problem:?}");
        let refused = Failure::refused(Some(Err(problem)), true);
        assert_eq!(refused, (!acceptable).then_some(Failure::Certificate(problem)), "marked: {problem:?}");
        assert_eq!(Failure::refused(Some(Err(problem)), false), Some(Failure::Certificate(problem)), "not marked: {problem:?}");
    }
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
    assert_eq!(stagenet.verdict(), Verdict::Working);
    let mainnet_node = check(Network::Stagenet, &node(Network::Mainnet.default_nodes()[0]), 0, now(), &|_| {}).await;
    assert_eq!(mainnet_node.verdict(), Verdict::Failed(Step::Network));
}

/// A node's Content-Length is only its word (audit #12): a huge one is
/// refused past the limit, and without one never has the phone set aside
/// what it says, which would abort the app.
#[tokio::test]
async fn a_huge_content_length_is_refused_not_allocated() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1099511627776\r\nConnection: close\r\n\r\n{}").await;
                let _ = stream.shutdown().await;
            });
        }
    });
    let transport = Transport::new(&format!("http://127.0.0.1:{port}"), false).unwrap();
    let limited = transport.post("get_info", vec![], Some(1024)).await;
    assert!(matches!(limited, Err(InterfaceError::InvalidInterface(_))), "{limited:?}");
    // No limit: the body ends short, and that's all.
    assert!(transport.post("get_info", vec![], None).await.is_err());
}

/// A self-signed node is trusted on first use (audit #13): the key it
/// answered with first is pinned, and a later handshake with another key
/// is refused as changed, the node marked or not.
#[tokio::test]
async fn a_self_signed_node_is_held_to_the_key_it_first_answered_with() {
    let first = canned(Canned::Tls { monerod: false, tls12: false }).await;
    let other = canned(Canned::Tls { monerod: true, tls12: false }).await;
    let url = |port: u16| format!("https://127.0.0.1:{port}");
    let fresh = Transport::pinned(&url(first), true, None).unwrap();
    assert_eq!(fresh.pin(), None, "nothing until a handshake");
    fresh.post("get_info", vec![], Some(1024)).await.unwrap();
    let pin = fresh.pin().expect("pinned on first use");
    // The same key again, on a new connection: accepted.
    let again = Transport::pinned(&url(first), true, Some(pin)).unwrap();
    again.post("get_info", vec![], Some(1024)).await.unwrap();
    assert_eq!(again.certificate(), Some(Err(CertProblem::SelfSigned)));
    // Another key where that one was: refused, and said so.
    let changed = Transport::pinned(&url(other), true, Some(pin)).unwrap();
    assert!(changed.post("get_info", vec![], Some(1024)).await.is_err());
    assert_eq!(changed.certificate(), Some(Err(CertProblem::Changed)));
    assert_eq!(Failure::refused(changed.certificate(), true), Some(Failure::Certificate(CertProblem::Changed)));
    assert_eq!(changed.pin(), Some(pin), "still held to the first");
}
