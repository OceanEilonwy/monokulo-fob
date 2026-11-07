mod addresses;
mod connection;
mod funds;
mod lifecycle;
mod manual_sync;
mod people;
mod prices;
mod send;
mod sync;

use super::*;

use crate::price_alert::Asked;

/// The phone's clock now, at UTC.
fn clock_now() -> Clock {
    Clock { now: now(), utc_offset: 0, hours_24: true }
}

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fob-wallet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("wallet.fob")
}

/// Leaves `file` at `path` as Fob kept wallets before the database, with no
/// database beside it: the next open migrates it.
fn old_file(path: &Path, file: &serde_json::Value) {
    crate::db::remove_files(&path.with_extension("db")).unwrap();
    store::save(&path.with_extension("fob"), &[1; 32], file).unwrap();
}

struct Quiet;

impl SyncListener for Quiet {
    fn on_status(&self, _: SyncStatus) {}
}

/// A wallet whose node refuses connections, so every round fails at once.
async fn unreachable(name: &str) -> Wallet {
    let wallet = Wallet::create(&dir(name), [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.node = "http://127.0.0.1:1".into();
    wallet
}

/// What the wallet's views say, for tests that read them as a person would.
pub(super) struct SaidLists {
    pub days: Vec<fob_words::activity::Day>,
    pub unsaved: Vec<fob_words::activity::PaidBefore>,
    pub on_send: Vec<fob_words::contacts::Row>,
    pub paid_before: Vec<fob_words::activity::PaidBefore>,
}

impl Wallet {
    pub(super) async fn said_lists(&self, clock: Clock) -> SaidLists {
        let lists = self.lists(clock).await;
        SaidLists {
            days: lists.days.into_iter().map(fob_words::activity::day).collect(),
            unsaved: lists.unsaved.into_iter().map(fob_words::activity::paid_before).collect(),
            on_send: lists.on_send.into_iter().map(fob_words::contacts::row).collect(),
            paid_before: lists.paid_before.into_iter().map(fob_words::activity::paid_before).collect(),
        }
    }

    pub(super) async fn said_coins(&self, clock: Clock) -> fob_words::not_ready::Coins {
        fob_words::not_ready::coins(self.coins(clock).await)
    }

    pub(super) async fn said_not_ready(&self, clock: Clock) -> Option<fob_words::not_ready::NotReady> {
        self.not_ready(clock).await.map(fob_words::not_ready::not_ready)
    }

    pub(super) async fn said_split_view(&self, counts: &[u32], clock: Clock) -> fob_words::split::View {
        fob_words::split::view(self.split_view(counts, clock).await)
    }

    pub(super) fn said_zmq(&self) -> String {
        let status = self.status();
        fob_words::catch_up::zmq_text(&status.zmq, status.zmq_endpoint.as_deref())
    }

    pub(super) async fn said_field(&self, text: &str, in_fiat: bool, clock: Clock) -> fob_words::prices::Field {
        fob_words::prices::field(self.amount_field(text, in_fiat, clock).await)
    }

    pub(super) async fn said_payment(&self, tx: &str, clock: Clock) -> Option<fob_words::activity::Detail> {
        self.payment(tx, clock).await.map(fob_words::activity::detail)
    }

    pub(super) async fn said_request(&self, id: u64, clock: Clock) -> Option<fob_words::requests::Detail> {
        self.request(id, clock).await.map(fob_words::requests::detail)
    }

    pub(super) async fn said_card(&self, id: u64, clock: Clock) -> Option<fob_words::contacts::Card> {
        self.contact_card(id, clock).await.map(fob_words::contacts::card)
    }

    pub(super) async fn said_offer(&self, tx: &str) -> Option<fob_words::contacts::Offer> {
        self.save_offer(tx).await.map(fob_words::contacts::offer)
    }

    pub(super) async fn said_profile_row(&self) -> fob_words::profile::SettingsRow {
        fob_words::profile::settings_row(self.profile_row().await)
    }

    pub(super) async fn said_shares(&self, clock: Clock) -> Vec<fob_words::profile::Line> {
        self.profile_shares(clock).await.into_iter().map(fob_words::profile::line).collect()
    }

    pub(super) async fn said_scanned(&self, text: &str) -> Result<fob_words::profile::Scanned> {
        self.scanned_profile(text).await.map(fob_words::profile::scanned)
    }

    pub(super) async fn said_contacts(&self, search: &str, clock: Clock) -> Vec<fob_words::contacts::Row> {
        self.contacts(search, clock).await.into_iter().map(fob_words::contacts::row).collect()
    }

    pub(super) async fn said_request_contacts(&self) -> Vec<fob_words::contacts::Row> {
        self.request_contacts().await.into_iter().map(fob_words::contacts::row).collect()
    }
}

/// Every part of a wallet that has been used, for the database's tests.
async fn a_used_wallet(path: &Path) -> Wallet {
    let wallet = Wallet::create(path, [1; 32], Network::Stagenet).unwrap();
    wallet.new_receive_address().await.unwrap();
    wallet.remember_receive_note(1, "Coffee beans").await.unwrap();
    wallet.add_node("https://mine.example:18081", true).await.unwrap();
    wallet.set_profile("Henry", Some(fob_codes::names::test_webp(300, 96))).await.unwrap();
    wallet.share_profile().await.unwrap();
    let sam = wallet.add_contact(contacts::Draft { name: "Sam".into(), note: Some("Neighbour".into()), photo: Some(fob_codes::names::test_webp(300, 96)), ..Default::default() }).await.unwrap();
    wallet.make_request(Some(sam.id), Some("0.5"), false, Some("Tickets")).await.unwrap();
    let mut state = wallet.inner.state.lock().await;
    state.outputs.push(Owned { subaddress: 1, coinbase: true, unlock_block: 77, key_image: Some("aa".into()), spent_in: Some("bb".into()), ..received("in", 1_700_000_000, u64::MAX) });
    state.outputs.push(received("in2", 1_700_000_100, XMR));
    state.spends.push(ChainSpend { tx: "bb".into(), height: 12, time: 1_700_000_200, fee: 30 });
    state.sent.push(Sent {
        not_confirmed: false,
        released: false,
        tx: "out".into(),
        to: "addr".into(),
        amount: XMR,
        fee: 30,
        change: 7,
        time: 1_700_000_300,
        height: Some(13),
        receipt: Some("{\"total\":\"1\"}".into()),
        note: Some("Rent".into()),
        pieces: Some(2),
        back: vec![3, 4],
        recipient_name: Some("Sam".into()),
        signed: None,
    });
    state.recent.push((13, "hash".into()));
    state.tip = 20;
    state.next_block = 14;
    state.restore_height = Some(1);
    state.split_index = Some(9);
    state.fresh_from = Some(4);
    state.prices.currency = Some("EUR".into());
    state.price_cache.current = Some(price::Current { currency: "NZD".into(), provider: "Kraken".into(), price: 166_000_000, at: 1_700_000_400 });
    state.price_cache.history.push(price::History { currency: "NZD".into(), provider: "Kraken".into(), fetched: 5, days: vec![(-1, 2), (3, 4)] });
    state.price_records.entry("in".into()).or_default().push(fiat::Record { currency: "NZD".into(), price: 1, provider: "Kraken".into(), at: 2, daily: true });
    state.price_records.entry("in".into()).or_default().push(fiat::Record { currency: "USD".into(), price: 3, provider: "CoinGecko".into(), at: 4, daily: false });
    wallet.save_state(&state).unwrap();
    drop(state);
    wallet
}

async fn saved(wallet: &Wallet) -> serde_json::Value {
    serde_json::to_value(&*wallet.inner.state.lock().await).unwrap()
}

/// How often a counting node was asked for the tip and for the fee.
#[derive(Default)]
struct NodeCalls {
    height: AtomicU64,
    fee: AtomicU64,
}

/// A stagenet node on 127.0.0.1 that answers only what a payment asks
/// first, counting each, with the tip taking `delay` to arrive so that
/// callers overlap. Gives its address and the counts.
async fn counting_node(delay: Duration) -> (String, Arc<NodeCalls>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let counts = Arc::new(NodeCalls::default());
    let seen = counts.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut request = vec![];
                let mut buf = [0; 4096];
                loop {
                    let Ok(n) = stream.read(&mut buf).await else { return };
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { continue };
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let len = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if request.len() >= end + 4 + len {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&request).into_owned();
                let body = if text.contains("get_height") {
                    seen.height.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(delay).await;
                    r#"{"height":3000000,"status":"OK"}"#
                } else if text.contains("on_get_block_hash") {
                    r#"{"jsonrpc":"2.0","id":0,"result":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
                } else if text.contains("get_fee_estimate") {
                    seen.fee.fetch_add(1, Ordering::Relaxed);
                    r#"{"jsonrpc":"2.0","id":0,"result":{"fee":20000,"quantization_mask":10000,"status":"OK"}}"#
                } else {
                    // Connecting asks something of json_rpc; any answer will do.
                    r#"{"jsonrpc":"2.0","id":0,"result":{}}"#
                };
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (url, counts)
}

/// A wallet using `node`, with an output distribution already kept up to
/// past the tip, so that only the tip and the fee are asked.
async fn wallet_for_sending(name: &str, node: &str) -> Wallet {
    let path = dir(name);
    let mut kept = 2_999_000u64.to_le_bytes().to_vec();
    kept.extend((0..2_000u64).flat_map(|i| (i * 10).to_le_bytes()));
    std::fs::write(path.with_file_name("wallet-stagenet.decoys"), kept).unwrap();
    let wallet = Wallet::create(&path, [1; 32], Network::Stagenet).unwrap();
    wallet.inner.state.lock().await.node = node.into();
    wallet
}

async fn ready(wallet: &Wallet) -> Result<(FeeRate, usize)> {
    let (_, rpc, decoys, _) = wallet.connection().await?;
    wallet.send_ready(&rpc, &decoys).await
}

/// A provider standing in for Kraken: a fixed price now, and a history.
struct FakePrices {
    now: u64,
    days: Vec<(i64, u64)>,
    fail: bool,
    asked_now: AtomicU64,
    asked_days: AtomicU64,
    /// Answers the current price only once this is notified.
    hold: Option<Arc<tokio::sync::Notify>>,
}

impl FakePrices {
    fn new(now: u64, days: Vec<(i64, u64)>) -> FakePrices {
        FakePrices { now, days, fail: false, asked_now: AtomicU64::new(0), asked_days: AtomicU64::new(0), hold: None }
    }

    fn asked(&self) -> (u64, u64) {
        (self.asked_now.load(Ordering::Relaxed), self.asked_days.load(Ordering::Relaxed))
    }
}

impl price::Source for FakePrices {
    fn name(&self) -> &'static str {
        "Kraken"
    }

    fn currencies(&self) -> &'static [&'static str] {
        &["USD", "EUR"]
    }

    fn answers_over(&self, transport: price::Transport) -> bool {
        transport == price::Transport::Web
    }

    async fn current(&self, _: &str, _: price::Transport) -> std::result::Result<u64, PriceError> {
        self.asked_now.fetch_add(1, Ordering::Relaxed);
        if let Some(hold) = &self.hold {
            hold.notified().await;
        }
        if self.fail { Err(PriceError::Client("no answer".into())) } else { Ok(self.now) }
    }

    async fn daily(&self, _: &str, days: std::ops::Range<i64>, _: price::Transport) -> std::result::Result<Vec<(i64, u64)>, PriceError> {
        self.asked_days.fetch_add(1, Ordering::Relaxed);
        if self.fail { Err(PriceError::Client("no answer".into())) } else { Ok(self.days.iter().copied().filter(|d| days.contains(&d.0)).collect()) }
    }

    fn history_days(&self) -> i64 {
        720
    }
}

const XMR: u64 = amount::ATOMIC_PER_XMR;

const DAY: u64 = 86_400;

fn received(tx: &str, time: u64, amount: u64) -> Owned {
    Owned {
        output: String::new(),
        tx: tx.into(),
        height: 10,
        time,
        amount,
        subaddress: 1,
        coinbase: false,
        unlock_block: 0,
        key: tx.into(),
        key_image: None,
        spent_in: None,
    }
}

fn price_of(activity: &[Activity], tx: &str) -> Option<(u64, bool)> {
    activity.iter().find(|a| a.tx == tx).and_then(|a| a.price.as_ref()).map(|p| (p.price, p.daily))
}

/// A payment spending the coins `received` made for each of `coins`.
fn outgoing(to: &str, coins: &[&str]) -> Outgoing {
    Outgoing {
        to: to.into(),
        amount: XMR / 10,
        inputs: coins.iter().map(|tx| ((*tx).to_owned(), String::new())).collect(),
        input_total: XMR,
        receipt: None,
        note: Some("Lunch".into()),
        recipient_name: Some("Sam".into()),
        pieces: None,
        back: vec![],
    }
}

fn webp(len: usize) -> Vec<u8> {
    fob_codes::names::test_webp(len, 96)
}

/// One touch between two phones' swaps: `reader` reads `card`.
fn touch(reader: &swap::Session, card: &swap::Session) -> swap::Step {
    let mut apdu = reader.reader_start().expect("still swapping");
    loop {
        match reader.reader_next(&card.card_answer(&apdu)) {
            swap::Step::Send(next) => apdu = next,
            step => return step,
        }
    }
}

/// Share my profile opened on `wallet`, and its swap.
async fn share_and_swap(wallet: &Wallet) -> (profile::Share, Arc<swap::Session>) {
    let (share, _) = wallet.share_profile().await.unwrap();
    let session = wallet.start_swap(share.id).await.unwrap();
    (share, session)
}
