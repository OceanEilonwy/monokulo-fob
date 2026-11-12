//! Accounts besides 0, each with a label, and the apps given view access to
//! one through Connect to Fob (decision 419).
//!
//! Account 0 is the wallet's own: Receive, requests, splits and change are
//! all there. An account made for an app ("Till (Corner Café)") is where that
//! app's payments land, so the wallet shows each one's takings apart. The
//! view key the app gets is the whole wallet's: forgetting an app removes
//! its line and the account's label, never its access.

use super::*;
use crate::connect;

/// The most characters an account's label may have.
pub const MAX_LABEL: usize = 64;

/// An account with a label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Its number: 1 or more.
    pub index: u32,
    pub label: String,
    /// When it was made, seconds since the epoch.
    pub created: u64,
}

/// An app connected through Connect to Fob.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connected {
    pub id: u64,
    /// The account made for it (or offered again on a reconnect).
    pub account: u32,
    /// The app's name as it asked ("Till") and what it called itself
    /// ("Corner Café"), and the device it asked from.
    pub app: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub device: String,
    /// When it was connected, seconds since the epoch.
    pub connected: u64,
}

/// An account as a screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountView {
    pub index: u32,
    /// `None` for account 0, the wallet's own, and for an account found
    /// paid that has no label here.
    pub label: Option<String>,
    pub balance: Balance,
    /// The apps connected to it.
    pub apps: Vec<Connected>,
}

/// A payment into one account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountPayment {
    pub tx: String,
    /// What came to the account in this transaction.
    pub amount: u64,
    /// Its block; `None` while in the pool.
    pub height: Option<u64>,
    pub confirmations: u64,
    /// When it was mined, or first seen in the pool.
    pub time: u64,
}

/// What a Connect to Fob answer handed over, besides the sealed answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Granted {
    /// The answer to open (a link) or show (a QR code).
    pub answer: String,
    pub account: u32,
    pub label: String,
}

/// What Give access can share besides the view key, for the consent
/// screen's switches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shares {
    /// The node's address, or the light server's when syncing through one.
    pub node: Option<String>,
    /// The price provider's name and the currency's code.
    pub prices: Option<(String, String)>,
}

fn clean_label(label: &str) -> Result<String> {
    let label = label.trim();
    if label.is_empty() {
        return Err(FobError::Screen(ScreenError::AccountLabelEmpty));
    }
    if label.chars().count() > MAX_LABEL {
        return Err(FobError::Screen(ScreenError::AccountLabelTooLong { max: MAX_LABEL }));
    }
    Ok(label.to_owned())
}

impl State {
    /// The next account no one has: past every labelled one, every one found
    /// paid, the receive account and every one an app was given.
    pub(super) fn next_account(&self) -> u32 {
        let used = self
            .accounts
            .iter()
            .map(|a| a.index)
            .chain(self.found_in.keys().copied())
            .chain(self.connections.iter().map(|c| c.account))
            .chain(self.keyed.iter().map(|r| r.account))
            .chain(std::iter::once(self.receive_account));
        used.max().unwrap_or(0).saturating_add(1).max(1)
    }

    fn label_of(&self, index: u32) -> Option<String> {
        self.accounts.iter().find(|a| a.index == index).map(|a| a.label.clone())
    }
}

impl Wallet {
    /// Fob's id for this wallet, made the first time it's asked: what a
    /// connected app knows it by.
    pub async fn wallet_id(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        if let Some(id) = &state.wallet_id {
            return Ok(id.clone());
        }
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let id = hex::encode(bytes);
        state.wallet_id = Some(id.clone());
        if let Err(e) = self.save_state(&state) {
            state.wallet_id = None;
            return Err(e);
        }
        Ok(id)
    }

    /// The account the next app (or [`add_account`](Wallet::add_account))
    /// gets.
    pub async fn next_account(&self) -> u32 {
        self.inner.state.lock().await.next_account()
    }

    /// A new account with `label`.
    pub async fn add_account(&self, label: &str) -> Result<Account> {
        let label = clean_label(label)?;
        let mut state = self.inner.state.lock().await;
        let account = Account { index: state.next_account(), label, created: now() };
        state.accounts.push(account.clone());
        if let Err(e) = self.save_state(&state) {
            state.accounts.pop();
            return Err(e);
        }
        drop(state);
        self.inner.wake.notify_one();
        Ok(account)
    }

    /// Gives account `index` (1 or more) a label, or a new one.
    pub async fn rename_account(&self, index: u32, label: &str) -> Result<()> {
        if index == 0 {
            return Err(FobError::Screen(ScreenError::NoSuchAccount));
        }
        let label = clean_label(label)?;
        let mut state = self.inner.state.lock().await;
        match state.accounts.iter_mut().find(|a| a.index == index) {
            Some(account) => account.label = label,
            None => state.accounts.push(Account { index, label, created: now() }),
        }
        self.save_state(&state)
    }

    /// Account 0, then every other account labelled or found paid, each
    /// with its balance and the apps connected to it.
    pub async fn accounts(&self) -> Vec<AccountView> {
        let indexes: Vec<u32> = {
            let state = self.inner.state.lock().await;
            let mut all: Vec<u32> = std::iter::once(0)
                .chain(state.accounts.iter().map(|a| a.index))
                .chain(state.found_in.keys().copied())
                .chain(state.connections.iter().map(|c| c.account))
                .collect();
            all.sort_unstable();
            all.dedup();
            all
        };
        let mut views = Vec::with_capacity(indexes.len());
        for index in indexes {
            let balance = self.account_balance(index).await;
            let state = self.inner.state.lock().await;
            views.push(AccountView {
                index,
                label: state.label_of(index),
                balance,
                apps: state.connections.iter().filter(|c| c.account == index).cloned().collect(),
            });
        }
        views
    }

    /// What account `index` holds, as [`balance`](Wallet::balance) counts it.
    /// Change from payments not yet mined is account 0's, where it comes
    /// back.
    pub async fn account_balance(&self, index: u32) -> Balance {
        let state = self.inner.state.lock().await;
        let device = self.inner.keys.device().is_some();
        let mut balance = Balance::default();
        for output in state.outputs.iter().filter(|o| o.account == index && o.coin.is_free()) {
            balance.total = balance.total.saturating_add(output.amount);
            if confirmations(&state, output.height) >= state.needed(output) {
                balance.unlocked = balance.unlocked.saturating_add(output.amount);
                if device && output.coin.key_image().is_none() {
                    balance.unsynced = balance.unsynced.saturating_add(output.amount);
                }
            }
        }
        if index == 0 {
            for sent in state.sent.iter().filter(|s| s.height.is_none() && !s.released) {
                balance.total = balance.total.saturating_add(sent.change);
            }
        }
        for pooled in self.inner.pool.lock().expect("pool lock").values() {
            let part = if index == 0 {
                amount::total(pooled.parts.iter().map(|p| p.1))
            } else {
                amount::total(pooled.accounts.iter().filter(|p| p.0 == index).map(|p| p.2))
            };
            balance.total = balance.total.saturating_add(part);
        }
        balance
    }

    /// The payments into account `index`, newest first, the pool's on top.
    /// Change coming back to account 0 is left out: it's no payment in.
    pub async fn account_activity(&self, index: u32) -> Vec<AccountPayment> {
        let state = self.inner.state.lock().await;
        let ours: HashSet<&str> = state.sent.iter().map(|s| s.tx.as_str()).chain(state.spends.iter().map(|s| s.tx.as_str())).collect();
        let mut rows: Vec<AccountPayment> = vec![];
        for output in state.outputs.iter().filter(|o| o.account == index && !ours.contains(o.tx.as_str())) {
            match rows.iter_mut().find(|r| r.tx == output.tx) {
                Some(row) => row.amount = row.amount.saturating_add(output.amount),
                None => rows.push(AccountPayment {
                    tx: output.tx.clone(),
                    amount: output.amount,
                    height: Some(output.height),
                    confirmations: confirmations(&state, output.height),
                    time: output.time,
                }),
            }
        }
        for (tx, pooled) in self.inner.pool.lock().expect("pool lock").iter() {
            let amount = if index == 0 {
                amount::total(pooled.parts.iter().map(|p| p.1))
            } else {
                amount::total(pooled.accounts.iter().filter(|p| p.0 == index).map(|p| p.2))
            };
            if amount > 0 && !rows.iter().any(|r| &r.tx == tx) && !ours.contains(tx.as_str()) {
                rows.push(AccountPayment { tx: tx.clone(), amount, height: None, confirmations: 0, time: pooled.seen });
            }
        }
        rows.sort_by(|a, b| (b.height.is_none(), b.height, b.time, &b.tx).cmp(&(a.height.is_none(), a.height, a.time, &a.tx)));
        rows
    }

    /// The apps connected to this wallet, oldest first.
    pub async fn connected_apps(&self) -> Vec<Connected> {
        self.inner.state.lock().await.connections.clone()
    }

    /// Forgets connected app `id`: its line, and its account's label unless
    /// another app still uses it. Its access stays: a view key can't be
    /// taken back.
    pub async fn forget_app(&self, id: u64) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let Some(at) = state.connections.iter().position(|c| c.id == id) else { return Ok(()) };
        let gone = state.connections.remove(at);
        if !state.connections.iter().any(|c| c.account == gone.account) {
            state.accounts.retain(|a| a.index != gone.account);
        }
        self.save_state(&state)
    }

    /// The account an app asking with `request` was given before (the same
    /// app and name), to offer again on a reconnect.
    pub async fn connected_before(&self, request: &connect::Request) -> Option<Connected> {
        let state = self.inner.state.lock().await;
        state.connections.iter().rev().find(|c| c.app == request.app && c.name == request.name).cloned()
    }

    /// What the consent screen's switches would share, as
    /// [`connect_app`](Wallet::connect_app) shares it: the node's or light
    /// server's address, and the price provider's name and currency (none
    /// without a currency, as nothing is shared then).
    pub async fn connect_shares(&self) -> Shares {
        let state = self.inner.state.lock().await;
        let node = if state.sync_mode == SyncMode::Light { state.light_url.clone() } else { Some(state.node.clone()).filter(|n| !n.is_empty()) };
        let prices = state.prices.currency.clone().map(|currency| (state.prices.provider.name().to_owned(), currency));
        Shares { node, prices }
    }

    /// Says yes to `request`: makes the account (or, with `account`, gives
    /// that one again, as on a reconnect) labelled `label`, remembers the
    /// app, and seals what it gets: the address and view key, network,
    /// account, label, restore height (the tip now) and this wallet's id
    /// and `wallet_name`, with this wallet's node or light server if
    /// `share_node` and the app asked, and its price source if
    /// `share_prices` and it asked.
    pub async fn connect_app(&self, request: &connect::Request, label: &str, account: Option<u32>, wallet_name: &str, share_node: bool, share_prices: bool) -> Result<Granted> {
        let network = self.network();
        if request.network != network {
            return Err(FobError::Monero(MoneroError::WrongNetwork { expected: network, found: request.network }));
        }
        let label = clean_label(label)?;
        let wallet_id = self.wallet_id().await?;
        let mut state = self.inner.state.lock().await;
        let index = account.filter(|a| *a > 0).unwrap_or_else(|| state.next_account());
        let before = (state.accounts.clone(), state.connections.clone());
        match state.accounts.iter_mut().find(|a| a.index == index) {
            Some(a) => a.label = label.clone(),
            None => state.accounts.push(Account { index, label: label.clone(), created: now() }),
        }
        let id = state.connections.iter().map(|c| c.id + 1).max().unwrap_or(1);
        state.connections.push(Connected { id, account: index, app: request.app.clone(), name: request.name.clone(), device: request.device.clone(), connected: now() });
        let restore_height = if state.tip > 0 { state.tip } else { network.restore_height_for(now()) };
        let light = state.sync_mode == SyncMode::Light;
        let shared = |url: &str, self_signed: bool, pin: &Option<String>| connect::SharedNode { url: url.to_owned(), self_signed, pin: pin.clone().filter(|_| self_signed) };
        let give_node = share_node && request.want.node;
        let grant = connect::Grant {
            address: self.inner.keys.primary_address(),
            view_key: self.inner.keys.view_key_hex().to_string(),
            network: network.name().to_owned(),
            account: index,
            label: label.clone(),
            restore_height,
            wallet_id,
            wallet_name: wallet_name.trim().to_owned(),
            node: (give_node && !light && !state.node.is_empty()).then(|| shared(&state.node, state.node_self_signed, &state.node_pin)),
            light: (give_node && light).then(|| state.light_url.as_deref().map(|url| shared(url, state.light_self_signed, &state.light_pin))).flatten(),
            prices: (share_prices && request.want.prices)
                .then(|| state.prices.currency.clone().map(|currency| connect::SharedPrices { provider: state.prices.provider.name().to_owned(), currency }))
                .flatten(),
        };
        let answer = match connect::seal(request, &grant) {
            Ok(answer) => answer,
            Err(_) => {
                (state.accounts, state.connections) = before;
                return Err(FobError::Code(CodeError::BadCode));
            }
        };
        if let Err(e) = self.save_state(&state) {
            (state.accounts, state.connections) = before;
            return Err(e);
        }
        drop(state);
        self.inner.wake.notify_one();
        Ok(Granted { answer, account: index, label })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_trimmed_and_neither_blank_nor_too_long() {
        assert_eq!(clean_label("  Till (Corner Café) ").unwrap(), "Till (Corner Café)");
        assert_eq!(clean_label(" "), Err(FobError::Screen(ScreenError::AccountLabelEmpty)));
        assert_eq!(clean_label(&"x".repeat(MAX_LABEL + 1)), Err(FobError::Screen(ScreenError::AccountLabelTooLong { max: MAX_LABEL })));
    }
}
