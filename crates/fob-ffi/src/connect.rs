//! Connect to Fob and accounts, for Fob's screens: reading a request,
//! listing the wallets to choose from, the account an app gets, and the
//! sealed answer (decisions 417, 419). The screens themselves are the
//! app's; every rule is here and below.

use std::sync::Arc;

use fob_wallet::{amount, connect};

use crate::{FobError, FobWallet, Network, QrCode, Result, WalletList};

fn invalid(e: connect::ConnectError) -> FobError {
    FobError::Invalid { reason: e.to_string() }
}

/// The pattern both screens draw for a request: 5×5 cells, row by row,
/// `true` in ink, on paper. The same in light and dark themes.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Identicon {
    pub cells: Vec<bool>,
    /// `#rrggbb`.
    pub ink: String,
    pub paper: String,
}

impl From<connect::Identicon> for Identicon {
    fn from(icon: connect::Identicon) -> Self {
        let swatch = icon.swatch();
        Identicon { cells: icon.cells.iter().flatten().copied().collect(), ink: swatch.ink.into(), paper: swatch.paper.into() }
    }
}

/// A Connect to Fob request, as the choose and consent screens show it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ConnectRequest {
    /// The asking app's name ("Till") and what it calls its account
    /// ("Corner Café"; may be blank), and the device asking.
    pub app: String,
    pub name: String,
    pub device: String,
    pub network: Network,
    /// The label Fob suggests for the account: "Till (Corner Café)".
    pub label: String,
    /// Whether it asks for the node and the price source too, for the
    /// consent screen's switches.
    pub wants_node: bool,
    pub wants_prices: bool,
    /// Whether the answer goes back by a link (the same device); without,
    /// Fob shows it as a QR code for the other device to scan.
    pub has_callback: bool,
    pub identicon: Identicon,
}

/// Whether `text` (a scanned code, a link opened) is a Connect to Fob request.
#[uniffi::export]
pub fn is_connect_request(text: String) -> bool {
    connect::Request::looks_like(&text)
}

/// Reads a Connect to Fob request; throws why it isn't one Fob can answer.
#[uniffi::export]
pub fn connect_request(text: String) -> Result<ConnectRequest> {
    let request = connect::Request::parse(&text).map_err(invalid)?;
    Ok(ConnectRequest {
        label: request.label(),
        identicon: request.identicon().into(),
        app: request.app,
        name: request.name,
        device: request.device,
        network: request.network.into(),
        wants_node: request.want.node,
        wants_prices: request.want.prices,
        has_callback: request.callback.is_some(),
    })
}

/// The answer for "Not now": a link to open, or with no callback the text
/// of a QR code.
#[uniffi::export]
pub fn connect_declined(text: String) -> Result<String> {
    Ok(connect::decline(&connect::Request::parse(&text).map_err(invalid)?))
}

/// A wallet on Choose a wallet, from the list alone (none is opened).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ConnectWallet {
    pub file: String,
    pub name: String,
    pub network: Network,
    /// The apps connected to it, by label: "N connected", and the first few
    /// under the chosen one.
    pub apps: Vec<String>,
    /// The account a new app would get: "New account: Account 13 · …".
    pub next_account: u32,
}

#[uniffi::export]
impl WalletList {
    /// The wallets an app on `network` can be connected to, in the list's
    /// order. "New wallet" is the screen's own.
    pub fn connect_wallets(&self, network: Network) -> Vec<ConnectWallet> {
        let network = fob_wallet::Network::from(network);
        self.book()
            .list()
            .iter()
            .filter(|e| e.network == network)
            .map(|e| ConnectWallet { file: e.file.clone(), name: e.name.clone(), network: e.network.into(), apps: e.apps.clone(), next_account: e.next_account })
            .collect()
    }

    /// Notes what `wallet` (open, in `file`) now says of its connected apps,
    /// after one is connected or forgotten.
    pub async fn note_connections(&self, file: String, wallet: Arc<FobWallet>) -> Result<()> {
        let (apps, next) = wallet.summary().await;
        Ok(self.book().set_apps(&file, apps, next)?)
    }
}

/// An app connected to a wallet.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ConnectedApp {
    pub id: u64,
    pub account: u32,
    pub app: String,
    pub name: String,
    pub device: String,
    /// Its account's label: "Till (Corner Café)".
    pub label: Option<String>,
    /// When, seconds since the epoch.
    pub connected: u64,
}

/// An account, with what it holds and who's connected to it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AccountRow {
    /// 0 is the wallet's own.
    pub index: u32,
    pub label: Option<String>,
    pub total: u64,
    pub unlocked: u64,
    pub total_text: String,
    pub unlocked_text: String,
    pub apps: Vec<ConnectedApp>,
}

/// A payment into one account.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AccountPaymentRow {
    pub tx: String,
    pub amount: u64,
    pub amount_text: String,
    /// `None` while it's in the pool.
    pub height: Option<u64>,
    pub confirmations: u64,
    pub time: u64,
}

/// What Give access made.
#[derive(uniffi::Record)]
pub struct ConnectAnswer {
    /// The sealed answer: a link to open on the same device, or the text of
    /// the QR code the other device scans.
    pub answer: String,
    pub qr: QrCode,
    /// Whether to open the answer as a link (the request had a callback)
    /// rather than show its code.
    pub open_link: bool,
    pub account: u32,
    pub label: String,
}

fn app_row(app: fob_wallet::Connected, label: Option<String>) -> ConnectedApp {
    ConnectedApp { id: app.id, account: app.account, app: app.app, name: app.name, device: app.device, label, connected: app.connected }
}

impl FobWallet {
    /// The connected apps' labels, and the account the next one gets.
    async fn summary(&self) -> (Vec<String>, u32) {
        let accounts = self.inner.accounts().await;
        let label = |index: u32| accounts.iter().find(|a| a.index == index).and_then(|a| a.label.clone());
        let apps = self.inner.connected_apps().await.into_iter().map(|c| label(c.account).unwrap_or(c.app)).collect();
        (apps, self.inner.next_account().await)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FobWallet {
    /// Every account: 0 first, then each labelled, paid or connected.
    pub async fn accounts(&self) -> Vec<AccountRow> {
        self.inner
            .accounts()
            .await
            .into_iter()
            .map(|a| AccountRow {
                index: a.index,
                total_text: amount::format_short(a.balance.total),
                unlocked_text: amount::format_short(a.balance.unlocked),
                total: a.balance.total,
                unlocked: a.balance.unlocked,
                apps: a.apps.into_iter().map(|c| app_row(c, a.label.clone())).collect(),
                label: a.label,
            })
            .collect()
    }

    /// The account the next app, or a new account, gets.
    pub async fn next_account(&self) -> u32 {
        self.inner.next_account().await
    }

    /// A new account named `label`; its number.
    pub async fn add_account(&self, label: String) -> Result<u32> {
        Ok(self.inner.add_account(&label).await?.index)
    }

    pub async fn rename_account(&self, index: u32, label: String) -> Result<()> {
        Ok(self.inner.rename_account(index, &label).await?)
    }

    /// The payments into account `index`, newest first.
    pub async fn account_activity(&self, index: u32) -> Vec<AccountPaymentRow> {
        self.inner
            .account_activity(index)
            .await
            .into_iter()
            .map(|p| AccountPaymentRow { amount_text: amount::format_short(p.amount), tx: p.tx, amount: p.amount, height: p.height, confirmations: p.confirmations, time: p.time })
            .collect()
    }

    /// Settings → Connected apps.
    pub async fn connected_apps(&self) -> Vec<ConnectedApp> {
        let accounts = self.inner.accounts().await;
        self.inner
            .connected_apps()
            .await
            .into_iter()
            .map(|c| {
                let label = accounts.iter().find(|a| a.index == c.account).and_then(|a| a.label.clone());
                app_row(c, label)
            })
            .collect()
    }

    /// Forget: the app's line and, once no app uses it, its account's label.
    /// Its view access stays.
    pub async fn forget_app(&self, id: u64) -> Result<()> {
        Ok(self.inner.forget_app(id).await?)
    }

    /// The app this request's app was before, on a reconnect: its account
    /// is offered again.
    pub async fn connected_before(&self, text: String) -> Result<Option<ConnectedApp>> {
        let request = connect::Request::parse(&text).map_err(invalid)?;
        let accounts = self.inner.accounts().await;
        Ok(self.inner.connected_before(&request).await.map(|c| {
            let label = accounts.iter().find(|a| a.index == c.account).and_then(|a| a.label.clone());
            app_row(c, label)
        }))
    }

    /// Give access: makes the account labelled `label` (or gives `account`
    /// again, on a reconnect), remembers the app, and seals the answer, with
    /// the node and price source when the switches are on. `wallet_name` is
    /// the wallet's name in the list, which the app shows.
    pub async fn connect_app(&self, text: String, label: String, account: Option<u32>, wallet_name: String, share_node: bool, share_prices: bool) -> Result<ConnectAnswer> {
        let request = connect::Request::parse(&text).map_err(invalid)?;
        let granted = self.inner.connect_app(&request, &label, account, &wallet_name, share_node, share_prices).await?;
        let qr = connect::answer_qr(&granted.answer).map_err(invalid)?;
        Ok(ConnectAnswer { qr: QrCode { size: qr.size, dark: qr.dark }, open_link: request.callback.is_some(), answer: granted.answer, account: granted.account, label: granted.label })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_for_the_screens() {
        let asker = connect::Asker::new("Till", "Corner Café", "iPad", fob_wallet::Network::Stagenet, connect::Want { node: true, prices: false }, None).unwrap();
        let text = asker.request().to_uri();
        assert!(is_connect_request(text.clone()));
        let read = connect_request(text.clone()).unwrap();
        assert_eq!((read.app.as_str(), read.label.as_str(), read.network), ("Till", "Till (Corner Café)", Network::Stagenet));
        assert!(read.wants_node && !read.wants_prices && !read.has_callback);
        assert_eq!(read.identicon.cells.len(), 25);
        assert!(read.identicon.ink.starts_with('#'));
        assert_eq!(asker.open(&connect_declined(text).unwrap()), Ok(connect::Answer::Declined));
        assert!(matches!(connect_request("monero:4abc".into()), Err(FobError::Invalid { .. })));
    }
}
