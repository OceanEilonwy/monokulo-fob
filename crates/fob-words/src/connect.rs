//! Connect to Fob, in words: Choose a wallet, the new wallet's words, the
//! consent screen, the pattern check and the sealed answer of the two-device
//! way, and Settings › Connected apps (decisions 417 to 420, 422).
//!
//! Nothing here is Till's or a shop's own: the asking app's name and device
//! come from its request, so any app that speaks the protocol gets the same
//! screens.

/// Over every screen of the flow: "Connect Till".
pub fn title(app: &str) -> String {
    format!("Connect {app}")
}

/// Choose a wallet's heading: "Choose a wallet for Till".
pub fn choose(app: &str) -> String {
    format!("Choose a wallet for {app}")
}

/// The first row, which makes one.
pub const NEW_WALLET: &str = "New wallet";

/// Under a wallet's name: "12 connected"; nothing for none.
pub fn connected(n: usize) -> Option<String> {
    (n > 0).then(|| format!("{n} connected"))
}

/// Over the account the chosen wallet would make.
pub const NEW_ACCOUNT: &str = "NEW ACCOUNT";

/// "Account 13 · Till (Corner Café)".
pub fn new_account(account: u32, label: &str) -> String {
    format!("Account {account} · {label}")
}

/// Over the apps already connected to the chosen wallet.
pub const CONNECTED_APPS: &str = "CONNECTED APPS";

/// How many of them the chosen wallet lists before "and N more".
pub const APPS_SHOWN: usize = 3;

/// After the first [`APPS_SHOWN`]: "and 9 more".
pub fn more(n: usize) -> String {
    format!("and {n} more")
}

pub const CONTINUE: &str = "Continue";
pub const NOT_NOW: &str = "Not now";

/// The asking app's line beside its name: the device it's on, or what the
/// request says when it named none.
pub fn device_line(device: &str) -> String {
    if device.trim().is_empty() { "This device".into() } else { device.trim().to_owned() }
}

/// A new wallet's screen.
pub const NEW_WALLET_TITLE: &str = "New wallet";
pub const WALLET_NAME: &str = "Wallet name";

/// "Write down its 16 words first".
pub fn write_down(words: usize) -> String {
    format!("Write down its {words} words first")
}

/// Why, naming the app that never sees them.
pub fn why_write(app: &str) -> String {
    format!("They're the only way back to this wallet's money if this {} is lost. {app} never sees them.", crate::device().noun())
}

pub const SHOW_WORDS: &str = "Show the words";
pub const WRITTEN: &str = "I've written the words down";
pub const MAKE_WALLET: &str = "Make the wallet";

/// The consent screen's question: "Give Till access to view payments?".
pub fn give_access(app: &str) -> String {
    format!("Give {app} access to view payments?")
}

/// Under it, the wallet's name to be shown bold:
/// "Till on Pixel 8 gets its own account in " **"Corner Café"** ".".
pub fn gets_account(app: &str, device: &str) -> (String, String) {
    let who = if device.trim().is_empty() { app.to_owned() } else { format!("{app} on {}", device.trim()) };
    (format!("{who} gets its own account in "), ".".into())
}

pub const ACCOUNT_NAME: &str = "Account name";

/// The two rows of what it gets: "Till can see money this wallet receives".
pub fn can_see(app: &str) -> String {
    format!("{app} can see money this wallet receives")
}

/// "Till can't spend anything".
pub fn cant_spend(app: &str) -> String {
    format!("{app} can't spend anything")
}

/// Only for a wallet that looks personal (money of its own in account 0,
/// no app connected yet): the view key is the whole wallet's.
pub fn wallet_wide(app: &str) -> String {
    format!("{app} can see payments to every account in this wallet, not only its own. A new wallet keeps them apart.")
}

pub const SHARE_NODE: &str = "Share my node";
pub const SHARE_PRICES: &str = "Share my price source";

/// The node switch's line: the node's address without its scheme
/// ("node.example.com:18089").
pub fn node_line(url: &str) -> String {
    url.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_owned()
}

/// The price switch's line: "Kraken · AUD".
pub fn prices_line(provider: &str, currency: &str) -> String {
    format!("{provider} · {currency}")
}

/// On a reconnect: give the account it had again.
pub const USE_AGAIN: &str = "Use its account again";

pub const GIVE: &str = "Give access";

/// What the other device is called, from the request's device name: "the
/// iPad" for "Corner Café iPad", "the other device" for a name that says
/// nothing.
pub fn other_device(device: &str) -> String {
    const KINDS: [&str; 8] = ["iPad", "iPhone", "Mac", "tablet", "phone", "laptop", "computer", "till"];
    let lower = device.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .rev()
        .find_map(|word| KINDS.iter().find(|k| k.to_lowercase() == word))
        .map(|kind| format!("the {kind}"))
        .unwrap_or_else(|| "the other device".into())
}

/// The scanned request's card: "Till on Corner Café iPad".
pub fn from_device(app: &str, device: &str) -> String {
    if device.trim().is_empty() { app.to_owned() } else { format!("{app} on {}", device.trim()) }
}

pub const FROM_CODE: &str = "From the code you scanned";

/// "Does the iPad show this pattern?".
pub fn same_pattern(device: &str) -> String {
    format!("Does {} show this pattern?", other_device(device))
}

/// "If they're different, the code isn't the one on your iPad. Stop here."
pub fn different(device: &str) -> String {
    let other = other_device(device);
    let yours = other.strip_prefix("the ").map(|kind| format!("your {kind}")).unwrap_or(other);
    format!("If they're different, the code isn't the one on {yours}. Stop here.")
}

pub const MATCH: &str = "They match";
pub const NO_MATCH: &str = "They don't match";

/// The sealed answer's heading: "Show this to the iPad".
pub fn show_this(device: &str) -> String {
    format!("Show this to {}", other_device(device))
}

/// "On the iPad, tap Scan Fob's code. Only that iPad can open this code."
pub fn show_how(device: &str) -> String {
    let other = other_device(device);
    let that = other.strip_prefix("the ").map(|kind| format!("that {kind}")).unwrap_or(other.clone());
    format!("On {other}, tap Scan Fob's code. Only {that} can open this code.")
}

/// Beside the small pattern: "For the iPad showing this pattern".
pub fn for_pattern(device: &str) -> String {
    format!("For {} showing this pattern", other_device(device))
}

pub const DONE: &str = "Done";

/// Settings › Connected apps.
pub const APPS_TITLE: &str = "Connected apps";

/// Settings' row: "Till (Corner Café)", "2 apps", or none.
pub fn apps_row(n: usize, first: Option<&str>) -> String {
    match (n, first) {
        (0, _) => "None".into(),
        (1, Some(label)) => label.to_owned(),
        (n, _) => format!("{n} apps"),
    }
}

/// Over the list, what it is.
pub const APPS_INTRO: &str = "Apps you gave access to view this wallet's payments. Each has an account of its own.";

/// With none.
pub const NO_APPS: &str = "No app can view this wallet's payments yet.";

/// An app's line: "Corner Café iPad · connected 12 Nov".
pub fn app_line(device: &str, date: &str) -> String {
    if device.trim().is_empty() { format!("Connected {date}") } else { format!("{} · connected {date}", device.trim()) }
}

/// An account's line: "Account 2 · 0.31 XMR".
pub fn account_line(account: u32, total: &str) -> String {
    format!("Account {account} · {total}")
}

pub const SWEEP: &str = "Sweep to main";
pub const FORGET: &str = "Forget";

/// Forget's question: "Forget Till (Corner Café)?".
pub fn forget_title(label: &str) -> String {
    format!("Forget {label}?")
}

/// What Forget does and doesn't.
pub fn forget_text(app: &str) -> String {
    format!(
        "Fob stops listing {app} and drops its account's name. {app} can still see this wallet's payments: a view key can't be taken back. To stop it, move the money to a new wallet."
    )
}

pub const CANCEL: &str = "Cancel";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choose_screen_counts_and_names_the_account() {
        assert_eq!(choose("Till"), "Choose a wallet for Till");
        assert_eq!(connected(0), None);
        assert_eq!(connected(12).as_deref(), Some("12 connected"));
        assert_eq!(new_account(13, "Till (Corner Café)"), "Account 13 · Till (Corner Café)");
        assert_eq!(more(9), "and 9 more");
        assert_eq!(device_line(" "), "This device");
    }

    #[test]
    fn the_other_device_is_named_from_the_request() {
        assert_eq!(other_device("Corner Café iPad"), "the iPad");
        assert_eq!(other_device("Front counter"), "the other device");
        assert_eq!(other_device("Sam's iPhone"), "the iPhone");
        assert_eq!(same_pattern("Corner Café iPad"), "Does the iPad show this pattern?");
        assert_eq!(different("Corner Café iPad"), "If they're different, the code isn't the one on your iPad. Stop here.");
        assert_eq!(different(""), "If they're different, the code isn't the one on your other device. Stop here.");
        assert_eq!(show_how("Corner Café iPad"), "On the iPad, tap Scan Fob's code. Only that iPad can open this code.");
        assert_eq!(for_pattern("Corner Café iPad"), "For the iPad showing this pattern");
    }

    #[test]
    fn consent_and_connected_apps_read_as_the_designs() {
        assert_eq!(give_access("Till"), "Give Till access to view payments?");
        assert_eq!(gets_account("Till", "Pixel 8").0, "Till on Pixel 8 gets its own account in ");
        assert_eq!(gets_account("Till", "").0, "Till gets its own account in ");
        assert_eq!(node_line("https://node.example.com:18089/"), "node.example.com:18089");
        assert_eq!(prices_line("Kraken", "AUD"), "Kraken · AUD");
        assert_eq!(apps_row(0, None), "None");
        assert_eq!(apps_row(1, Some("Till (Corner Café)")), "Till (Corner Café)");
        assert_eq!(apps_row(3, Some("Till (Corner Café)")), "3 apps");
        assert_eq!(app_line("Corner Café iPad", "12 Nov"), "Corner Café iPad · connected 12 Nov");
        assert!(forget_text("Till").contains("a view key can't be taken back"));
    }
}
