use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

/// A server on 127.0.0.1 answering each request with the status and body
/// `answer` gives for its route and body. Gives its address and every
/// request's route and body as it came.
async fn fake_server(answer: impl Fn(&str, &str) -> (u16, String) + Send + Sync + 'static) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let (answer, log) = (answer.clone(), log.clone());
            tokio::spawn(async move {
                let mut request = vec![];
                let mut buf = [0; 4096];
                let (head, body) = loop {
                    let Ok(n) = stream.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&request[..end]).into_owned();
                    let len = head.to_ascii_lowercase().lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break (head, String::from_utf8_lossy(&request[end + 4..end + 4 + len]).into_owned());
                    }
                };
                let route = head.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_owned();
                log.lock().unwrap().push((route.clone(), body.clone()));
                let (status, text) = answer(&route, &body);
                let response = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (url, seen)
}

fn login() -> Login {
    Login { address: "5abc".into(), view_key: Hex([7; 32]) }
}

const VERSION: &str = r#"{"server_type":"monokulo-lws","monero_version_full":"0.18","blockchain_height":2250000,"api":2,"max_subaddresses":10000,"network":"stage","testnet":true}"#;

#[tokio::test]
async fn login_answers_200_403_and_501() {
    let (url, seen) = fake_server(|route, body| match (route, body.contains("\"create_account\":true")) {
        ("login", true) => (200, r#"{"new_address":true,"generated_locally":true,"start_height":2249999,"lookahead":{"maj_i":1,"min_i":200}}"#.into()),
        ("login", false) => (403, "{}".into()),
        _ => (404, String::new()),
    })
    .await;
    let client = LightClient::new(&url, false).unwrap();
    let made = client.login(&login(), true, true).await.unwrap();
    assert!(made.new_address);
    assert_eq!(made.start_height, Some(2_249_999));
    assert_eq!(client.login(&login(), false, true).await.unwrap_err(), Problem::Forbidden);
    let (route, body) = seen.lock().unwrap()[0].clone();
    assert_eq!(route, "login");
    let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(sent["view_key"], hex::encode([7; 32]), "the view key travels in the body");
    assert_eq!(sent["generated_locally"], true);

    let (closed, _) = fake_server(|_, _| (501, r#"{"message":"account creation is not allowed"}"#.into())).await;
    let client = LightClient::new(&closed, false).unwrap();
    assert_eq!(client.login(&login(), true, false).await.unwrap_err(), Problem::NotTaken);
}

#[tokio::test]
async fn subaddresses_off_is_a_conflict_with_the_server_s_words() {
    let (url, seen) = fake_server(|route, _| match route {
        "upsert_subaddrs" => (409, r#"{"message":"subaddresses are off on this server"}"#.into()),
        _ => (404, String::new()),
    })
    .await;
    let client = LightClient::new(&url, false).unwrap();
    let ranges = vec![SubaddrRanges { key: 0, value: vec![[1, 200]] }];
    assert_eq!(client.upsert_subaddrs(&login(), ranges).await.unwrap_err(), Problem::Conflict("subaddresses are off on this server".into()));
    let sent: serde_json::Value = serde_json::from_str(&seen.lock().unwrap()[0].1).unwrap();
    assert_eq!(sent["subaddrs"], serde_json::json!([{"key": 0, "value": [[1, 200]]}]));
}

#[tokio::test]
async fn other_statuses_and_garbage() {
    let (url, _) = fake_server(|route, _| match route {
        "get_address_txs" => (503, String::new()),
        "get_address_info" => (200, "not json".into()),
        "get_unspent_outs" => (400, r#"{"message":"insufficient funds"}"#.into()),
        _ => (418, String::new()),
    })
    .await;
    let client = LightClient::new(&url, false).unwrap();
    assert_eq!(client.get_address_txs(&login()).await.unwrap_err(), Problem::Unavailable);
    assert_eq!(client.get_address_info(&login()).await.unwrap_err(), Problem::Unreadable);
    let asked = UnspentOutsRequest { login: login(), amount: Amount(5), mixin: 15, use_dust: true, dust_threshold: None };
    assert_eq!(client.get_unspent_outs(&asked).await.unwrap_err(), Problem::BadRequest("insufficient funds".into()));
    assert_eq!(client.get_subaddrs(&login()).await.unwrap_err(), Problem::Status(418));
}

#[tokio::test]
async fn get_version_sends_no_key_and_check_judges_it() {
    let (url, seen) = fake_server(|route, _| match route {
        "get_version" => (200, VERSION.into()),
        _ => (404, String::new()),
    })
    .await;
    let client = LightClient::new(&url, false).unwrap();
    let version = client.get_version().await.unwrap();
    assert_eq!((version.api, version.network.as_str(), version.max_subaddresses), (2, "stage", 10_000));
    assert!(!seen.lock().unwrap()[0].1.contains("view_key"));

    let report = check(Network::Stagenet, &url, false, None).await;
    assert_eq!(report.refusal(), None);
    assert_eq!(report.address, url.trim_start_matches("http://"));
    let report = check(Network::Mainnet, &url, false, None).await;
    assert_eq!(report.refusal(), Some(Problem::OtherNetwork { theirs: "stage".into(), ours: "main".into() }));

    let (none, _) = fake_server(|_, _| (200, VERSION.replace("10000", "0"))).await;
    assert_eq!(check(Network::Stagenet, &none, false, None).await.refusal(), Some(Problem::NoSubaddresses));
}

#[tokio::test]
async fn an_unreachable_or_onion_server_is_said_so() {
    let report = check(Network::Stagenet, "http://127.0.0.1:1", false, None).await;
    assert!(matches!(report.refusal(), Some(Problem::Unreachable(_))));
    assert_eq!(LightClient::new("http://lws.abcdefghijklmnop.onion:8443", false).err(), Some(Problem::Onion));
    assert_eq!(check(Network::Stagenet, "lws.example.onion.", false, None).await.refusal(), Some(Problem::Onion));
}

/// The view key goes with every request: `https://` only, but for a server
/// on this device. A bare `host:port` is taken as `https://`.
#[test]
fn plain_http_only_to_this_device() {
    assert_eq!(LightClient::new("http://lws.example:8443", false).err(), Some(Problem::PlainHttp));
    assert_eq!(LightClient::new("http://192.168.1.20:8443", false).err(), Some(Problem::PlainHttp));
    assert!(LightClient::new("https://lws.example:8443", false).is_ok());
    assert!(LightClient::new("http://127.0.0.1:18443", false).is_ok());
    assert!(LightClient::new("http://localhost:18443", false).is_ok());
    assert!(LightClient::new("http://[::1]:18443", false).is_ok());
    assert_eq!(super::super::normalize("lws.example:8443"), "https://lws.example:8443");
    assert_eq!(super::super::normalize("127.0.0.1:18443/"), "http://127.0.0.1:18443");
    assert_eq!(LightClient::new("lws.abcdefghijklmnop.i2p", false).err(), Some(Problem::Onion));
    assert_eq!(LightClient::new("ftp://lws.example", false).err(), Some(Problem::BadAddress));
}

/// An answer past [`LIMIT`] is refused before it's read, by the length
/// the server says, and the poll fails.
#[tokio::test]
async fn an_answer_too_large_is_refused() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf).await;
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{{", LIMIT + 1);
                let _ = stream.write_all(head.as_bytes()).await;
            });
        }
    });
    let client = LightClient::new(&url, false).unwrap();
    assert_eq!(client.get_address_txs(&login()).await.unwrap_err(), Problem::Unreachable("the server's answer was too large".into()));
}

/// A transaction the server's node took is "OK"; one it turned down comes
/// back as monero-lws and monokulo-lws send it, a 422 with the reason,
/// which is kept; a 422 without a readable body is still a refusal.
#[tokio::test]
async fn a_submitted_transaction_is_taken_or_refused_with_the_reason() {
    let (url, seen) = fake_server(|route, body| match (route, body.contains("\"tx\":\"0102\"")) {
        ("submit_raw_tx", true) => (200, r#"{"status":"OK","not_relayed":false}"#.into()),
        ("submit_raw_tx", false) if body.contains("\"tx\":\"03\"") => (422, r#"{"status":"Failed","reason":"double spend","not_relayed":true}"#.into()),
        ("submit_raw_tx", false) => (422, "not json".into()),
        _ => (404, String::new()),
    })
    .await;
    let client = LightClient::new(&url, false).unwrap();
    let taken = client.submit_raw_tx(vec![1, 2]).await.unwrap();
    assert_eq!((taken.status.as_str(), taken.not_relayed), ("OK", Some(false)));
    let refused = client.submit_raw_tx(vec![3]).await.unwrap();
    assert_eq!((refused.status.as_str(), refused.reason.as_deref()), ("Failed", Some("double spend")));
    let unreadable = client.submit_raw_tx(vec![4]).await.unwrap();
    assert_eq!((unreadable.status.as_str(), unreadable.reason), ("Failed", None));
    assert_eq!(seen.lock().unwrap()[0].0, "submit_raw_tx");
}
