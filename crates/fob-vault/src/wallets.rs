//! The wallets on this phone: each one's name and network, and which is open.
//!
//! Every wallet is a database of its own in one folder, `wallet-<network>.db`
//! for one from before Fob held several, `wallet-<network>-<id>.db` since. One
//! not opened since the database came is still `.fob`, a sealed file, which
//! becomes a database as it opens and is then deleted. The list is sealed too, and the files are
//! what counts: a wallet the list doesn't know joins it under a name of its
//! own, and an entry whose files have gone leaves it. So a list that's lost, or
//! a crash between making a wallet and listing it, costs a name and never a
//! wallet.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{error::VaultError, error::Result, store};
use fob_monero::network::Network;

const LIST: &str = "wallets.list";
/// The most characters a wallet's name may have.
const NAME_LENGTH: usize = 32;

const NETWORKS: [Network; 2] = [Network::Mainnet, Network::Stagenet];

/// A wallet on this phone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The wallet's file name without its extension, which never changes.
    pub file: String,
    pub name: String,
    pub network: Network,
}

#[derive(Default, Serialize, Deserialize)]
struct List {
    wallets: Vec<Entry>,
    /// The file last open on each network, by network name.
    current: HashMap<String, String>,
}

/// The list of wallets, kept in `dir`.
pub struct Wallets {
    dir: PathBuf,
    key: Zeroizing<[u8; 32]>,
    list: List,
}

/// The network a wallet's name says it's on, if it is a wallet's.
fn network_of(name: &str) -> Option<Network> {
    NETWORKS.into_iter().find(|n| {
        let plain = format!("wallet-{}", n.name());
        name == plain || name.strip_prefix(&plain).is_some_and(|rest| rest.len() > 1 && rest.starts_with('-'))
    })
}

/// Every wallet in `dir`, by its file name without the extension, once each
/// though a migrated one has both a database and a sealed file.
fn files_in(dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter_map(|name| name.strip_suffix(".db").or(name.strip_suffix(".fob")).map(str::to_owned))
                .filter(|name| network_of(name).is_some())
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.dedup();
    files
}

/// Removes every file of the wallet whose database is at `path`, those that
/// exist: its database, and the sealed file it may have come from.
pub fn remove_files(path: &Path) -> Result<()> {
    crate::db::remove_files(&path.with_extension("db"))?;
    remove_sealed_file(path)
}

/// Removes the sealed file of the wallet at `path`, and any temporary one a
/// crash left while it was being written: both hold the seed.
pub fn remove_sealed_file(path: &Path) -> Result<()> {
    for extension in ["tmp", "fob"] {
        match std::fs::remove_file(path.with_extension(extension)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(VaultError::Storage(e.to_string())),
            _ => {}
        }
    }
    Ok(())
}

/// Whether `dir` holds a wallet on `network`. Asks nothing of the keystore, so
/// it can be asked while the first screen is chosen.
pub fn any_on(dir: &Path, network: Network) -> bool {
    files_in(dir).iter().any(|file| network_of(file) == Some(network))
}

/// `name` as it's kept, or why it can't be.
fn clean_name(name: &str) -> Result<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(VaultError::NameEmpty);
    }
    if name.chars().count() > NAME_LENGTH {
        return Err(VaultError::NameTooLong { max: NAME_LENGTH });
    }
    Ok(name)
}

impl Wallets {
    /// The list in `dir`, brought into line with the files there.
    pub fn open(dir: &Path, key: [u8; 32]) -> Result<Wallets> {
        // A list that can't be read is made again from the files.
        let list = store::load(&dir.join(LIST), &key).unwrap_or_default();
        let mut wallets = Wallets { dir: dir.to_owned(), key: Zeroizing::new(key), list };
        if wallets.reconcile() {
            wallets.save()?;
        }
        Ok(wallets)
    }

    /// Lists files the list doesn't know and drops entries whose file has
    /// gone. Whether anything changed.
    fn reconcile(&mut self) -> bool {
        let files = files_in(&self.dir);
        let before = self.list.wallets.len();
        self.list.wallets.retain(|w| files.contains(&w.file));
        let mut changed = self.list.wallets.len() != before;
        for file in files {
            if self.list.wallets.iter().any(|w| w.file == file) {
                continue;
            }
            let network = network_of(&file).expect("files_in keeps only wallet files");
            let name = self.next_name();
            self.list.wallets.push(Entry { file, name, network });
            changed = true;
        }
        let known: Vec<String> = self.list.wallets.iter().map(|w| w.file.clone()).collect();
        let current = self.list.current.len();
        self.list.current.retain(|_, file| known.contains(file));
        changed || self.list.current.len() != current
    }

    fn save(&self) -> Result<()> {
        store::save(&self.dir.join(LIST), &self.key, &self.list)
    }

    /// Every wallet, in the order they joined.
    pub fn list(&self) -> &[Entry] {
        &self.list.wallets
    }

    fn find(&self, file: &str) -> Option<&Entry> {
        self.list.wallets.iter().find(|w| w.file == file)
    }

    /// The wallet to open on `network`: the last one open there, else the
    /// first there is.
    pub fn current(&self, network: Network) -> Option<&Entry> {
        self.list
            .current
            .get(network.name())
            .and_then(|file| self.find(file))
            .filter(|w| w.network == network)
            .or_else(|| self.list.wallets.iter().find(|w| w.network == network))
    }

    /// Remembers `file` as the one open on its network.
    pub fn select(&mut self, file: &str) -> Result<()> {
        let entry = self.find(file).ok_or(VaultError::NotOnThisDevice)?;
        let network = entry.network.name().to_owned();
        self.list.current.insert(network, file.to_owned());
        self.save()
    }

    /// The path of a wallet's database.
    pub fn path(&self, file: &str) -> PathBuf {
        self.dir.join(format!("{file}.db"))
    }

    /// The name a new wallet gets unless it's given one: "Wallet 2".
    pub fn next_name(&self) -> String {
        let taken = |name: &str| self.list.wallets.iter().any(|w| w.name == name);
        (self.list.wallets.len() + 1..).map(|n| format!("Wallet {n}")).find(|name| !taken(name)).expect("the numbers don't run out")
    }

    /// A file name no wallet has, for a new wallet on `network`.
    pub fn new_file(&self, network: Network) -> String {
        let mut id = [0u8; 8];
        OsRng.fill_bytes(&mut id);
        format!("wallet-{}-{}", network.name(), hex::encode(id))
    }

    /// Checks a new wallet's `name` before it's made ("" takes the next name).
    pub fn check_name(&self, name: &str) -> Result<String> {
        if name.trim().is_empty() { Ok(self.next_name()) } else { self.unique(name, None) }
    }

    fn unique(&self, name: &str, except: Option<&str>) -> Result<String> {
        let name = clean_name(name)?;
        if self.list.wallets.iter().any(|w| w.name.eq_ignore_ascii_case(&name) && Some(w.file.as_str()) != except) {
            return Err(VaultError::NameTaken { name: name.to_string() });
        }
        Ok(name)
    }

    /// Lists a wallet just made in `file`, as the one open on its network.
    pub fn add(&mut self, file: &str, name: &str) -> Result<Entry> {
        let network = network_of(file).ok_or(VaultError::NotAWalletFileName)?;
        let name = self.check_name(name)?;
        let entry = Entry { file: file.to_owned(), name, network };
        self.list.wallets.retain(|w| w.file != file);
        self.list.wallets.push(entry.clone());
        self.list.current.insert(network.name().to_owned(), file.to_owned());
        self.save()?;
        Ok(entry)
    }

    /// Gives a wallet a new name.
    pub fn rename(&mut self, file: &str, name: &str) -> Result<()> {
        let name = self.unique(name, Some(file))?;
        let entry = self.list.wallets.iter_mut().find(|w| w.file == file).ok_or(VaultError::NotOnThisDevice)?;
        entry.name = name;
        self.save()
    }

    /// Takes a wallet off the phone: its file, and its line on the list.
    pub fn remove(&mut self, file: &str) -> Result<()> {
        // Only a wallet this list holds, by the name it holds it under.
        if self.find(file).is_some() {
            remove_files(&self.path(file))?;
            self.list.wallets.retain(|w| w.file != file);
            self.list.current.retain(|_, current| current != file);
            self.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fob-wallets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A wallet's database, as far as the list can tell.
    fn touch(dir: &Path, file: &str) {
        std::fs::write(dir.join(format!("{file}.db")), b"sealed").unwrap();
    }

    const KEY: [u8; 32] = [3; 32];

    #[test]
    fn wallets_from_before_there_was_a_list_join_it_where_they_are() {
        let d = dir("old");
        // One still a sealed file, one migrated with its file kept: one wallet each.
        std::fs::write(d.join("wallet-mainnet.fob"), b"sealed").unwrap();
        std::fs::write(d.join("wallet-stagenet.fob"), b"sealed").unwrap();
        touch(&d, "wallet-stagenet");
        for other in ["wallet-mainnet.decoys", "wallet-mainnet.tmp", "wallet-mainnet.db.migrating", "wallet-stagenet.db-wal"] {
            std::fs::write(d.join(other), b"other").unwrap();
        }
        let wallets = Wallets::open(&d, KEY).unwrap();
        let listed: Vec<(&str, &str, Network)> = wallets.list().iter().map(|w| (w.file.as_str(), w.name.as_str(), w.network)).collect();
        assert_eq!(listed, [("wallet-mainnet", "Wallet 1", Network::Mainnet), ("wallet-stagenet", "Wallet 2", Network::Stagenet)]);
        assert_eq!(wallets.current(Network::Mainnet).unwrap().file, "wallet-mainnet");
        assert_eq!(wallets.path("wallet-mainnet"), d.join("wallet-mainnet.db"));
        assert!(any_on(&d, Network::Mainnet), "a sealed file is a wallet too");
        assert!(d.join("wallet-mainnet.fob").exists(), "left where it was");
    }

    #[test]
    fn names_and_the_wallet_last_open_outlive_the_run() {
        let d = dir("keep");
        let mut wallets = Wallets::open(&d, KEY).unwrap();
        assert_eq!(wallets.next_name(), "Wallet 1");
        let first = wallets.new_file(Network::Mainnet);
        touch(&d, &first);
        wallets.add(&first, "").unwrap();
        let second = wallets.new_file(Network::Mainnet);
        assert_ne!(first, second);
        touch(&d, &second);
        wallets.add(&second, "  Savings   jar ").unwrap();
        assert_eq!(wallets.current(Network::Mainnet).unwrap().name, "Savings jar", "the newest is the one open");
        wallets.select(&first).unwrap();
        wallets.rename(&first, "Spending").unwrap();

        let wallets = Wallets::open(&d, KEY).unwrap();
        assert_eq!(wallets.current(Network::Mainnet).unwrap().name, "Spending");
        assert_eq!(wallets.list().len(), 2);
        assert_eq!(wallets.current(Network::Stagenet), None);
    }

    #[test]
    fn names_must_be_there_short_enough_and_not_taken() {
        let d = dir("names");
        let mut wallets = Wallets::open(&d, KEY).unwrap();
        let (a, b) = (wallets.new_file(Network::Mainnet), wallets.new_file(Network::Stagenet));
        touch(&d, &a);
        touch(&d, &b);
        wallets.add(&a, "Spending").unwrap();
        wallets.add(&b, "Savings").unwrap();
        assert!(matches!(wallets.rename(&b, "   "), Err(VaultError::NameEmpty)));
        assert!(matches!(wallets.rename(&b, "SPENDING"), Err(VaultError::NameTaken { .. })));
        assert!(matches!(wallets.check_name("spending"), Err(VaultError::NameTaken { .. })));
        assert!(matches!(wallets.rename(&b, &"x".repeat(NAME_LENGTH + 1)), Err(VaultError::NameTooLong { .. })));
        wallets.rename(&b, "savings").unwrap();
        assert_eq!(wallets.list()[1].name, "savings", "a wallet may change its own name's case");
    }

    #[test]
    fn a_removed_wallet_is_gone_and_another_takes_its_place() {
        let d = dir("remove");
        let mut wallets = Wallets::open(&d, KEY).unwrap();
        let (a, b) = (wallets.new_file(Network::Mainnet), wallets.new_file(Network::Mainnet));
        touch(&d, &a);
        touch(&d, &b);
        wallets.add(&a, "One").unwrap();
        wallets.add(&b, "Two").unwrap();
        wallets.remove(&b).unwrap();
        assert!(!d.join(format!("{b}.db")).exists());
        assert_eq!(wallets.current(Network::Mainnet).unwrap().name, "One");
        assert!(wallets.remove("../outside.fob").is_ok() && wallets.list().len() == 1, "only what's on the list");
        wallets.remove(&a).unwrap();
        assert_eq!(wallets.current(Network::Mainnet), None);
        assert!(!any_on(&d, Network::Mainnet));
    }

    #[test]
    fn a_lost_list_costs_names_not_wallets() {
        let d = dir("lost");
        let mut wallets = Wallets::open(&d, KEY).unwrap();
        let a = wallets.new_file(Network::Stagenet);
        touch(&d, &a);
        wallets.add(&a, "Testing").unwrap();
        std::fs::write(d.join(LIST), b"garbage").unwrap();
        let wallets = Wallets::open(&d, KEY).unwrap();
        assert_eq!((wallets.list()[0].file.as_str(), wallets.list()[0].name.as_str()), (a.as_str(), "Wallet 1"));

        // A file gone behind the list's back leaves it.
        std::fs::remove_file(d.join(format!("{a}.db"))).unwrap();
        assert!(Wallets::open(&d, KEY).unwrap().list().is_empty());
    }

    #[test]
    fn only_wallets_count() {
        assert_eq!(network_of("wallet-mainnet"), Some(Network::Mainnet));
        assert_eq!(network_of("wallet-stagenet-ab12"), Some(Network::Stagenet));
        for other in ["wallets", "wallet-mainnetish", "wallet-mainnet-", "wallet-", "other"] {
            assert_eq!(network_of(other), None, "{other}");
        }
    }

    #[test]
    fn removing_a_wallet_takes_its_old_file_too() {
        let d = dir("both");
        std::fs::write(d.join("wallet-mainnet.fob"), b"sealed").unwrap();
        touch(&d, "wallet-mainnet");
        std::fs::write(d.join("wallet-mainnet.db-wal"), b"log").unwrap();
        let mut wallets = Wallets::open(&d, KEY).unwrap();
        wallets.remove("wallet-mainnet").unwrap();
        for gone in ["wallet-mainnet.fob", "wallet-mainnet.db", "wallet-mainnet.db-wal"] {
            assert!(!d.join(gone).exists(), "{gone}");
        }
    }
}
