//! Keyed requests, accounts and Connect to Fob, through the wallet
//! (decisions 418, 419).

use super::*;
use crate::connect::{self, Answer, Asker, Want};
use super::keyed::{self, Ask, DOUBLE_SPEND_BLOCKS, POOL_SIZE, TAG_MODULUS};

const XMR: u64 = 1_000_000_000_000;

fn ask(key: &str, atomic: u64) -> Ask {
    Ask { key: key.into(), atomic, expires_at: now() + 1800, note: None }
}

/// An output of `amount` to `minor` of `account`, mined at `height`.
fn paid_at(tx: &str, account: u32, minor: u32, amount: u64, height: u64) -> Owned {
    Owned { account, subaddress: minor, height, time: now(), ..received(tx, now(), amount) }
}

#[derive(Default)]
struct Heard(std::sync::Mutex<Vec<Vec<u64>>>);

impl SyncListener for Heard {
    fn on_status(&self, _: SyncStatus) {}
    fn on_requests(&self, ids: Vec<u64>) {
        self.0.lock().unwrap().push(ids);
    }
}

#[tokio::test]
async fn a_keyed_request_needs_a_receive_account_and_an_amount() {
    let wallet = Wallet::create(&dir("keyed-needs"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.request_payment(ask("a", XMR)).await, Err(FobError::Screen(ScreenError::NoReceiveAccount)));
    wallet.set_receive_account(3).await.unwrap();
    assert_eq!(wallet.receive_account().await, 3);
    assert_eq!(wallet.request_payment(ask("a", 0)).await, Err(FobError::Screen(ScreenError::RequestNeedsAmount)));
}

#[tokio::test]
async fn the_same_key_gives_the_same_request() {
    let wallet = Wallet::create(&dir("keyed-same"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_receive_account(3).await.unwrap();
    let first = wallet.request_payment(Ask { note: Some(" Coffee ".into()), ..ask("sale-1", 2 * XMR) }).await.unwrap();
    assert_eq!((first.account, first.minor), (3, 1));
    assert_eq!(first.address, wallet.inner.keys.address_at(3, 1));
    assert_eq!(first.base, 2 * XMR);
    assert!(first.tag < TAG_MODULUS);
    assert_eq!(first.atomic, 2 * XMR + first.tag);
    assert_eq!(first.note.as_deref(), Some("Coffee"));
    assert!(first.uri().contains(&format!("tx_amount={}", amount::format(first.atomic))), "{}", first.uri());
    // Asked again: the same request, the same address and tag.
    assert_eq!(wallet.request_payment(ask("sale-1", 2 * XMR)).await.unwrap(), first);
    assert_eq!(wallet.request_payment(ask("sale-1", 3 * XMR)).await, Err(FobError::Screen(ScreenError::RequestKeyTaken)));
    // Another key: the next address in the pool, another tag.
    let second = wallet.request_payment(ask("sale-2", 2 * XMR)).await.unwrap();
    assert_eq!(second.minor, 2);
    assert_ne!(second.id, first.id);
    assert_eq!(wallet.keyed_request_by_key("sale-2").await, Some(second.clone()));
    // Fob's own Receive is untouched: account 0, no address handed out.
    assert_eq!(wallet.inner.state.lock().await.receive_index, 0);
    // All of it survives the file.
    let path = wallet.inner.path.clone();
    drop(wallet);
    let back = Wallet::open(&path, [1; 32]).unwrap();
    assert_eq!(back.receive_account().await, 3);
    assert_eq!(back.keyed_requests().await, vec![first.clone(), second]);
    assert_eq!(back.request_payment(ask("sale-1", 2 * XMR)).await.unwrap(), first);
}

#[tokio::test]
async fn a_full_pool_grows_and_a_cancelled_request_frees_its_address_a_day_later() {
    let wallet = Wallet::create(&dir("keyed-pool"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_receive_account(2).await.unwrap();
    for i in 0..POOL_SIZE {
        wallet.request_payment(ask(&format!("k{i}"), XMR)).await.unwrap();
    }
    let extra = wallet.request_payment(ask("one more", XMR)).await.unwrap();
    assert_eq!(extra.minor, POOL_SIZE + 1);
    // Cancelled, the address waits out the late window.
    wallet.cancel_request(1).await.unwrap();
    let cancelled = wallet.keyed_request(1).await.unwrap();
    assert!(cancelled.cancelled.is_some());
    assert!(cancelled.releases() >= now() + keyed::LATE_WINDOW - 5);
    // A day on (moved back by hand), it's the first free.
    wallet.inner.state.lock().await.keyed[0].cancelled = Some(now() - keyed::LATE_WINDOW - 1);
    assert_eq!(wallet.request_payment(ask("reuse", XMR)).await.unwrap().minor, 1);
    // Scanning watches the grown pool and a margin past it.
    let plan = wallet.inner.state.lock().await.watch_plan();
    assert_eq!(plan.upto(2), Some(POOL_SIZE + 1 + keyed::POOL_MARGIN));
    assert_eq!(plan.upto(0), Some(Wallet::watched(0)));
    assert_eq!(wallet.cancel_request(99).await, Err(FobError::Screen(ScreenError::NoSuchRequest)));
}

#[tokio::test]
async fn payments_are_matched_by_address_and_amount() {
    let wallet = Wallet::create(&dir("keyed-match"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_receive_account(3).await.unwrap();
    let a = wallet.request_payment(ask("a", 2 * XMR)).await.unwrap();
    let b = wallet.request_payment(ask("b", 5 * XMR)).await.unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 120;
        // Exactly a's amount, at a's address, mined at 100: 20 confirmations.
        state.outputs.push(paid_at("exact", 3, a.minor, a.atomic, 100));
        // Less than b asked, b the one open there: partial.
        state.outputs.push(paid_at("part", 3, b.minor, XMR, 110));
        // Another wallet's customer on a's address: a foreign tag.
        state.outputs.push(paid_at("theirs", 3, a.minor, 7 * XMR + 424_242, 111));
        // Money elsewhere in the account is none of theirs.
        state.outputs.push(paid_at("elsewhere", 3, 40, 9 * XMR, 112));
    }
    // b's rest, still in the pool.
    wallet.pool_found_at([("pooled".to_owned(), (3, b.minor), b.atomic - XMR)], now());
    let paid_a = wallet.request_payments(a.id).await.unwrap();
    assert_eq!(paid_a, vec![keyed::Incoming { tx: "exact".into(), atomic: a.atomic, height: Some(100), confirmations: 20, double_spent: false, exact: true }]);
    let paid_b = wallet.request_payments(b.id).await.unwrap();
    assert_eq!(paid_b.len(), 2);
    assert_eq!((paid_b[0].tx.as_str(), paid_b[0].exact, paid_b[0].confirmations), ("part", false, 10));
    // The pool's rest is less than asked, so partial too; no height yet.
    assert_eq!((paid_b[1].tx.as_str(), paid_b[1].height, paid_b[1].exact), ("pooled", None, false));
    let unexpected = wallet.unexpected_payments().await;
    assert_eq!(unexpected.len(), 1);
    assert_eq!((unexpected[0].tx.as_str(), unexpected[0].foreign_tag, unexpected[0].minor), ("theirs", true, a.minor));
    assert_eq!(wallet.request_payments(99).await, Err(FobError::Screen(ScreenError::NoSuchRequest)));
    // The account's own view: everything to account 3.
    assert_eq!(wallet.account_balance(3).await.total, a.atomic + XMR + 7 * XMR + 424_242 + 9 * XMR + b.atomic - XMR);
    assert_eq!(wallet.account_balance(0).await.total, 0);
    let activity = wallet.account_activity(3).await;
    assert_eq!(activity.first().map(|p| (p.tx.as_str(), p.height)), Some(("pooled", None)));
    assert_eq!(activity.len(), 5);
}

#[tokio::test]
async fn a_pool_payment_that_never_comes_back_is_double_spent() {
    let wallet = Wallet::create(&dir("keyed-double"), [1; 32], Network::Stagenet).unwrap();
    wallet.set_receive_account(1).await.unwrap();
    let r = wallet.request_payment(ask("r", XMR)).await.unwrap();
    wallet.inner.state.lock().await.tip = 500;
    wallet.pool_found_at([("vanishing".to_owned(), (1, r.minor), r.atomic)], now());
    assert!(!wallet.request_payments(r.id).await.unwrap()[0].double_spent);
    // It leaves the pool unmined.
    wallet.inner.pool.lock().unwrap().clear();
    let gone = wallet.request_payments(r.id).await.unwrap();
    assert_eq!((gone.len(), gone[0].double_spent), (1, false), "not yet: it may be mined in a block not scanned");
    wallet.inner.state.lock().await.tip = 500 + DOUBLE_SPEND_BLOCKS;
    assert!(wallet.request_payments(r.id).await.unwrap()[0].double_spent);
    // Mined after all: it's the block's, not double spent.
    wallet.inner.state.lock().await.outputs.push(paid_at("vanishing", 1, r.minor, r.atomic, 505));
    let mined = wallet.request_payments(r.id).await.unwrap();
    assert_eq!((mined.len(), mined[0].height, mined[0].double_spent), (1, Some(505), false));
}

#[tokio::test]
async fn the_listener_hears_which_requests_changed() {
    let wallet = Wallet::create(&dir("keyed-heard"), [1; 32], Network::Stagenet).unwrap();
    let heard = Arc::new(Heard::default());
    wallet.listen(heard.clone());
    wallet.tell_requests().await;
    assert!(heard.0.lock().unwrap().is_empty(), "no requests, nothing to say");
    wallet.set_receive_account(1).await.unwrap();
    let a = wallet.request_payment(ask("a", XMR)).await.unwrap();
    let b = wallet.request_payment(ask("b", XMR)).await.unwrap();
    wallet.tell_requests().await;
    assert_eq!(heard.0.lock().unwrap().pop(), Some(vec![a.id, b.id]), "first told: each as it stands");
    wallet.tell_requests().await;
    assert!(heard.0.lock().unwrap().is_empty(), "nothing changed");
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 101;
        state.outputs.push(paid_at("t", 1, b.minor, b.atomic, 100));
    }
    wallet.tell_requests().await;
    assert_eq!(heard.0.lock().unwrap().pop(), Some(vec![b.id]));
    // A block more: one more confirmation is a change too.
    wallet.inner.state.lock().await.tip = 102;
    wallet.tell_requests().await;
    assert_eq!(heard.0.lock().unwrap().pop(), Some(vec![b.id]));
}

#[tokio::test]
async fn another_account_s_money_names_none_of_account_0_s_requests() {
    let wallet = Wallet::create(&dir("keyed-activity"), [1; 32], Network::Stagenet).unwrap();
    wallet.remember_receive_note(4, "Rent").await.unwrap();
    let till = wallet.add_account("Till (Corner Café)").await.unwrap();
    assert_eq!(till.index, 1);
    wallet.inner.state.lock().await.outputs.push(paid_at(&tx_hex("till"), 1, 4, XMR, 10));
    let activity = wallet.activity().await;
    let row = activity.iter().find(|a| a.tx == tx_hex("till")).unwrap();
    assert_eq!(row.note.as_deref(), Some("Till (Corner Café)"), "the account's label, not account 0's note at index 4");
    assert!(row.parts.is_empty());
    assert_eq!(row.amount, XMR);
}

#[tokio::test]
async fn accounts_have_labels_and_their_own_balances() {
    let wallet = Wallet::create(&dir("accounts"), [1; 32], Network::Stagenet).unwrap();
    assert_eq!(wallet.next_account().await, 1);
    let one = wallet.add_account(" Savings ").await.unwrap();
    assert_eq!((one.index, one.label.as_str()), (1, "Savings"));
    assert_eq!(wallet.add_account("").await, Err(FobError::Screen(ScreenError::AccountLabelEmpty)));
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 100;
        state.outputs.push(paid_at("main", 0, 1, 2 * XMR, 50));
        state.outputs.push(paid_at("found", 5, 0, XMR, 50));
        state.count_found();
    }
    // An account found paid counts as used.
    assert_eq!(wallet.next_account().await, 6);
    wallet.rename_account(5, "Old till").await.unwrap();
    assert_eq!(wallet.rename_account(0, "Main").await, Err(FobError::Screen(ScreenError::NoSuchAccount)));
    let views = wallet.accounts().await;
    let summary: Vec<(u32, Option<&str>, u64)> = views.iter().map(|v| (v.index, v.label.as_deref(), v.balance.total)).collect();
    assert_eq!(summary, vec![(0, None, 2 * XMR), (1, Some("Savings"), 0), (5, Some("Old till"), XMR)]);
    assert_eq!(wallet.account_balance(5).await.unlocked, XMR);
    // Scanning watches each labelled account.
    let plan = wallet.inner.state.lock().await.watch_plan();
    assert_eq!(plan.upto(1), Some(keyed::ACCOUNT_LOOKAHEAD));
}

#[tokio::test]
async fn connect_to_fob_makes_an_account_and_hands_over_the_view_key() {
    let wallet = Wallet::create(&dir("connect"), [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 2_000_000;
        state.node = "https://node.example:38081".into();
    }
    wallet.set_currency(Some("AUD")).await.ok();
    let asker = Asker::new("Till", "Corner Café", "Corner Café iPad", Network::Stagenet, Want { node: true, prices: true }, None).unwrap();
    let request = connect::Request::parse(&asker.request().to_uri()).unwrap();
    assert_eq!(wallet.connected_before(&request).await, None);
    // The consent screen's switches show what would be shared.
    let shares = wallet.connect_shares().await;
    assert_eq!(shares.node.as_deref(), Some("https://node.example:38081"));
    assert_eq!(shares.prices.as_ref().map(|(_, currency)| currency.as_str()), Some("AUD"));
    let granted = wallet.connect_app(&request, &request.label(), None, "Corner Café", true, true).await.unwrap();
    assert_eq!((granted.account, granted.label.as_str()), (1, "Till (Corner Café)"));
    let Answer::Granted(grant) = asker.open(&granted.answer).unwrap() else { panic!("granted") };
    assert_eq!(grant.address, wallet.primary_address());
    assert_eq!(grant.view_key, *wallet.inner.keys.view_key_hex());
    assert_eq!((grant.network.as_str(), grant.account, grant.restore_height), ("stagenet", 1, 2_000_000));
    assert_eq!(grant.wallet_id, wallet.wallet_id().await.unwrap());
    assert_eq!(grant.wallet_name, "Corner Café");
    assert_eq!(grant.node.as_ref().map(|n| n.url.as_str()), Some("https://node.example:38081"));
    assert!(grant.light.is_none());
    // The app's line, and the account's label.
    let apps = wallet.connected_apps().await;
    assert_eq!(apps.len(), 1);
    assert_eq!((apps[0].app.as_str(), apps[0].device.as_str(), apps[0].account), ("Till", "Corner Café iPad", 1));
    assert_eq!(wallet.connected_before(&request).await.map(|c| c.account), Some(1));
    assert_eq!(wallet.next_account().await, 2);
    // A second till gets the next account; a reconnect, the same one.
    let again = wallet.connect_app(&request, "Till (Front counter)", Some(1), "Corner Café", false, false).await.unwrap();
    assert_eq!(again.account, 1);
    let Answer::Granted(grant) = asker.open(&again.answer).unwrap() else { panic!("granted") };
    assert!(grant.node.is_none() && grant.prices.is_none(), "nothing shared when switched off");
    let other = Asker::new("Till", "Kiosk", "", Network::Stagenet, Want::default(), None).unwrap();
    assert_eq!(wallet.connect_app(other.request(), "Till (Kiosk)", None, "", true, true).await.unwrap().account, 2);
    // Another network's request is refused.
    let mainnet = Asker::new("Till", "", "", Network::Mainnet, Want::default(), None).unwrap();
    assert!(matches!(wallet.connect_app(mainnet.request(), "Till", None, "", true, true).await, Err(FobError::Monero(MoneroError::WrongNetwork { .. }))));
    // Forgetting an app takes its line and, once no app uses it, the label.
    let first = wallet.connected_apps().await[0].id;
    wallet.forget_app(first).await.unwrap();
    assert!(wallet.accounts().await.iter().any(|a| a.index == 1 && a.label.is_some()), "the reconnect still uses account 1");
    for app in wallet.connected_apps().await.into_iter().filter(|a| a.account == 1) {
        wallet.forget_app(app.id).await.unwrap();
    }
    assert!(!wallet.accounts().await.iter().any(|a| a.index == 1 && a.label.is_some()));
}

#[tokio::test]
async fn sweep_to_main_takes_only_the_account_s_coins_ready_to_spend() {
    let wallet = Wallet::create(&dir("sweep"), [1; 32], Network::Stagenet).unwrap();
    {
        let mut state = wallet.inner.state.lock().await;
        state.tip = 1_000;
        state.outputs.push(paid_at("main", 0, 1, 5 * XMR, 100));
        state.outputs.push(paid_at("till-old", 2, 3, 2 * XMR, 100).with_coin(Coin::Free { ki: KeyImage::Known([2; 32]) }));
        state.outputs.push(paid_at("till-new", 2, 4, XMR, 995).with_coin(Coin::Free { ki: KeyImage::Known([3; 32]) }));
        state.outputs.push(paid_at("other-till", 3, 1, 7 * XMR, 100).with_coin(Coin::Free { ki: KeyImage::Known([4; 32]) }));
    }
    // Made by hand, the outputs don't read as monero-oxide's: what counts
    // here is which are picked, so this checks the filter on the ids.
    let state = wallet.inner.state.lock().await;
    let ready: Vec<&str> = state.spendable_coins().iter().map(|(i, _)| &state.outputs[*i]).filter(|o| o.account == 2).map(|o| o.tx.as_str()).collect();
    assert_eq!(ready, ["till-old"], "the new coin isn't ready, and other accounts aren't swept");
    drop(state);
    assert_eq!(wallet.prepare_sweep(0).await.unwrap_err(), FobError::Screen(ScreenError::NoSuchAccount));
    assert!(matches!(wallet.prepare_sweep(5).await, Err(FobError::Screen(ScreenError::NotEnoughFunds { available: 0, .. }))));
    let full = Keys::from_words(Network::Stagenet, &wallet.seed_words().unwrap().1).unwrap().keys;
    let watching = Wallet::restore_watching(&dir("sweep-watch"), [1; 32], Keys::view_only(Network::Stagenet, &full.primary_address(), &full.view_key_hex()).unwrap(), None).unwrap();
    assert_eq!(watching.prepare_sweep(2).await.unwrap_err(), FobError::Vault(VaultError::ViewOnly));
    assert_eq!(Wallet::restore_watching(&dir("sweep-spend"), [1; 32], full, None).err(), Some(FobError::Vault(VaultError::BadKey)));
}
