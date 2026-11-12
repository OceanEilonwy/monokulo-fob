//! Requests made with an idempotency key, for an app using fob-wallet as a
//! library (Monokular's Till, decision 418).
//!
//! Each is in the wallet's receive account, on an address from a small pool
//! there, and asks for its amount plus a tag: `tag = H(tag id, key) mod
//! 10⁶` piconero, under a millionth of an XMR. Two payments on one address
//! are told apart by their amounts:
//!
//! - an output counts toward a request **exactly** when its amount is the
//!   request's, tag and all;
//! - **as a partial payment** when its amount is no request's, it's less
//!   than asked, and exactly one request was open on that address when it
//!   came;
//! - otherwise it's **unexpected**, and reported, with whether its tag is
//!   one this wallet never made (another wallet sharing the account).
//!
//! An address is free again once every request on it has ended (expired or
//! cancelled) and a late window of 24 hours has passed; a new request takes
//! the one free the longest, or one more index when none is. Fob's own
//! requests (`fob_screens::requests`) are untouched: each keeps a fresh
//! address in account 0 and no tag.

use super::*;
use sha3::{Digest, Sha3_256};
use std::collections::BTreeMap;

/// Addresses a receive account's pool starts with: indexes 1 to this.
pub const POOL_SIZE: u32 = 50;
/// Indexes watched past the pool's highest, in case another wallet sharing
/// the account has handed out a few more.
pub const POOL_MARGIN: u32 = 10;
/// How long an ended request still holds its address: a late payment is
/// still its own, never the next sale's.
pub const LATE_WINDOW: u64 = 24 * 3600;
/// Tags are below this, in piconero.
pub const TAG_MODULUS: u64 = 1_000_000;
/// A payment seen in the pool that left it unmined and hasn't come back
/// within this many blocks is called double spent.
pub const DOUBLE_SPEND_BLOCKS: u64 = 10;
/// Indexes watched past the highest paid in an account Fob made for an app:
/// the app hands out its own addresses there.
pub const ACCOUNT_LOOKAHEAD: u32 = 200;

/// A request to be paid, made with an idempotency key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyedRequest {
    pub id: u64,
    /// The caller's idempotency key: asking again with it gives this request.
    pub key: String,
    pub account: u32,
    /// The address's index in `account`.
    pub minor: u32,
    pub address: String,
    /// The amount asked before the tag, in piconero.
    pub base: u64,
    /// The tag added to it.
    pub tag: u64,
    /// What the code asks: `base + tag`.
    pub atomic: u64,
    /// When it was made and when it expires, seconds since the epoch.
    pub created: u64,
    pub expires_at: u64,
    /// The chain's height when it was made: nothing before it pays it.
    pub height: u64,
    /// When the caller called it off, if it did.
    #[serde(default)]
    pub cancelled: Option<u64>,
    /// The note its payment code carries (`tx_description`).
    #[serde(default)]
    pub note: Option<String>,
}

impl KeyedRequest {
    /// When it stopped being open: at expiry, or when called off before.
    pub fn ends(&self) -> u64 {
        self.cancelled.map_or(self.expires_at, |c| c.min(self.expires_at))
    }

    /// When its address is free again.
    pub fn releases(&self) -> u64 {
        self.ends().saturating_add(LATE_WINDOW)
    }

    /// Whether it was open at `time`.
    pub fn open_at(&self, time: u64) -> bool {
        self.created <= time && time <= self.ends()
    }

    /// Its payment code: `monero:<address>?tx_amount=<amount, tag and all>`.
    pub fn uri(&self) -> String {
        uri::payment_uri(&self.address, Some(self.atomic), self.note.as_deref(), None)
    }
}

/// What a new keyed request asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub key: String,
    /// In piconero, before the tag.
    pub atomic: u64,
    pub expires_at: u64,
    pub note: Option<String>,
}

/// A payment toward a keyed request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incoming {
    pub tx: String,
    /// What came to the request's address in this transaction.
    pub atomic: u64,
    /// The block it was mined in; `None` while it's in the pool (or left it).
    pub height: Option<u64>,
    pub confirmations: u64,
    /// It was seen in the pool, left unmined and hasn't come back within
    /// [`DOUBLE_SPEND_BLOCKS`] blocks.
    pub double_spent: bool,
    /// Its amount is the request's, tag and all; otherwise it's a partial
    /// payment.
    pub exact: bool,
}

/// An output to an address of a keyed request that counts toward none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unexpected {
    pub tx: String,
    pub account: u32,
    pub minor: u32,
    pub atomic: u64,
    pub height: Option<u64>,
    pub confirmations: u64,
    /// Its amount carries a tag this wallet never made: another wallet may
    /// be using the same account.
    pub foreign_tag: bool,
}

/// The amount tag for `key` in a wallet whose tag id is `tag_id`.
pub fn tag_for(tag_id: &str, key: &str) -> u64 {
    let mut hash = Sha3_256::new();
    hash.update(b"fob amount tag v1");
    hash.update((tag_id.len() as u64).to_le_bytes());
    hash.update(tag_id.as_bytes());
    hash.update(key.as_bytes());
    let digest = hash.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes")) % TAG_MODULUS
}

/// The index a new request in `account` takes at `now`: of the pool's
/// addresses free now (never used, or every request on it released), the
/// one free the longest; with none free, the one after the highest.
pub fn pick_minor(requests: &[KeyedRequest], account: u32, now: u64) -> u32 {
    let mut released: BTreeMap<u32, u64> = BTreeMap::new();
    for r in requests.iter().filter(|r| r.account == account) {
        let at = released.entry(r.minor).or_insert(0);
        *at = (*at).max(r.releases());
    }
    let highest = released.keys().next_back().copied().unwrap_or(0).max(POOL_SIZE);
    let free = (1..=highest.min(addresses::MOST_ADDRESSES)).filter_map(|i| {
        let at = released.get(&i).copied().unwrap_or(0);
        (at <= now).then_some((at, i))
    });
    match free.min() {
        Some((_, i)) => i,
        None => highest.saturating_add(1).min(addresses::MOST_ADDRESSES),
    }
}

/// An output seen at an address, in a block or the pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub tx: String,
    pub account: u32,
    pub minor: u32,
    pub atomic: u64,
    pub height: Option<u64>,
    /// When it was mined, or first seen in the pool.
    pub time: u64,
    pub double_spent: bool,
}

/// Outputs matched to keyed requests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matching {
    /// By request id, each output with whether it matched exactly.
    pub paid: HashMap<u64, Vec<(Seen, bool)>>,
    /// Those that count toward none, with whether their tag is foreign.
    pub unexpected: Vec<(Seen, bool)>,
}

/// Matches `seen` to `requests` by address and amount (see the module's
/// doc). Outputs of one transaction to one address count as one.
pub fn match_outputs(requests: &[KeyedRequest], seen: &[Seen]) -> Matching {
    let mut joined: Vec<Seen> = vec![];
    for s in seen {
        match joined.iter_mut().find(|j| j.tx == s.tx && j.account == s.account && j.minor == s.minor) {
            Some(j) => j.atomic = j.atomic.saturating_add(s.atomic),
            None => joined.push(s.clone()),
        }
    }
    let ours: HashSet<u64> = requests.iter().map(|r| r.tag).collect();
    let mut matching = Matching::default();
    for s in joined {
        let on: Vec<&KeyedRequest> = requests.iter().filter(|r| r.account == s.account && r.minor == s.minor).collect();
        if on.is_empty() {
            continue;
        }
        // Exactly: the newest made before it came, else the newest.
        let exact = on.iter().filter(|r| r.atomic == s.atomic).max_by_key(|r| (r.created <= s.time, r.created, r.id));
        if let Some(r) = exact {
            matching.paid.entry(r.id).or_default().push((s, true));
            continue;
        }
        let open: Vec<&&KeyedRequest> = on.iter().filter(|r| r.open_at(s.time)).collect();
        if let [r] = open[..]
            && s.atomic < r.atomic
        {
            matching.paid.entry(r.id).or_default().push((s, false));
            continue;
        }
        let tag = s.atomic % TAG_MODULUS;
        let foreign = tag != 0 && !ours.contains(&tag);
        matching.unexpected.push((s, foreign));
    }
    matching
}

/// What a pool payment to a keyed request's address was, kept after it
/// left the pool to tell a double spend.
#[derive(Clone, Debug)]
pub(super) struct Remembered {
    seen: Seen,
    /// The chain's height when it was first missed from the pool.
    left_at: Option<u64>,
}

/// How far scanning watches in each account: the highest index, by
/// account. Account 0's primary address is always watched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Watch(pub(super) BTreeMap<u32, u32>);

impl Watch {
    /// Account 0 up to `upto`, alone.
    #[cfg(test)]
    pub(super) fn first(upto: u32) -> Watch {
        Watch([(0, upto)].into_iter().collect())
    }

    /// Whether this watches at least as far as `other` everywhere.
    pub(super) fn covers(&self, other: &Watch) -> bool {
        other.0.iter().all(|(account, upto)| self.0.get(account).is_some_and(|mine| mine >= upto))
    }

    /// Each (account, index) watched that `before` didn't.
    pub(super) fn beyond<'a>(&'a self, before: &'a Watch) -> impl Iterator<Item = SubaddressIndex> + 'a {
        self.0.iter().flat_map(move |(&account, &upto)| {
            let from = match before.0.get(&account) {
                Some(&had) => had.saturating_add(1),
                None if account == 0 => 1,
                None => 0,
            };
            (from..=upto.min(addresses::MOST_ADDRESSES)).filter_map(move |i| SubaddressIndex::new(account, i))
        })
    }

    pub(super) fn upto(&self, account: u32) -> Option<u32> {
        self.0.get(&account).copied()
    }
}

impl State {
    /// How far scanning watches now: account 0 as Receive hands out; each
    /// account made for an app, [`ACCOUNT_LOOKAHEAD`] past its highest paid;
    /// the receive account, its pool and [`POOL_MARGIN`] past it; and any
    /// account found paid, past its highest.
    pub(super) fn watch_plan(&self) -> Watch {
        let mut plan = BTreeMap::new();
        plan.insert(0, Wallet::watched(self.receive_index));
        let mut grow = |account: u32, upto: u32| {
            let at = plan.entry(account).or_insert(0);
            *at = (*at).max(upto.min(addresses::MOST_ADDRESSES));
        };
        for (&account, &found) in &self.found_in {
            grow(account, found.saturating_add(ACCOUNT_LOOKAHEAD));
        }
        for account in &self.accounts {
            grow(account.index, self.found_in.get(&account.index).copied().unwrap_or(0).saturating_add(ACCOUNT_LOOKAHEAD));
        }
        if self.receive_account != 0 {
            let used = self.keyed.iter().filter(|r| r.account == self.receive_account).map(|r| r.minor).max().unwrap_or(0);
            grow(self.receive_account, used.max(POOL_SIZE).saturating_add(POOL_MARGIN));
        }
        for r in &self.keyed {
            grow(r.account, r.minor.saturating_add(POOL_MARGIN));
        }
        Watch(plan)
    }

    /// Notes a coin found at `minor` of `account`, for [`watch_plan`](State::watch_plan).
    pub(super) fn found_at(&mut self, account: u32, minor: u32) {
        if account != 0 {
            let at = self.found_in.entry(account).or_insert(0);
            *at = (*at).max(minor);
        }
    }

    /// Works out [`found_in`](State::found_in) from the coins kept.
    pub(super) fn count_found(&mut self) {
        self.found_in.clear();
        let found: Vec<(u32, u32)> = self.outputs.iter().map(|o| (o.account, o.subaddress)).collect();
        for (account, minor) in found {
            self.found_at(account, minor);
        }
    }
}

impl Wallet {
    /// The account keyed requests take their addresses in; 0 until set.
    pub async fn receive_account(&self) -> u32 {
        self.inner.state.lock().await.receive_account
    }

    /// Has keyed requests take addresses in account `n`, and scanning watch
    /// its pool (a light server is told at its next poll). 0 turns keyed
    /// requests off: account 0 is Fob's own, whose requests keep a fresh
    /// address each and no tag.
    pub async fn set_receive_account(&self, n: u32) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if state.receive_account == n {
            return Ok(());
        }
        let was = state.receive_account;
        state.receive_account = n;
        if let Err(e) = self.save_state(&state) {
            state.receive_account = was;
            return Err(e);
        }
        drop(state);
        self.inner.wake.notify_one();
        Ok(())
    }

    /// A request for `ask.atomic` plus this wallet's tag, on an address
    /// from the receive account's pool, expiring at `ask.expires_at`. Asked
    /// again with the same key, gives the same request; with the same key
    /// and another amount, refuses.
    pub async fn request_payment(&self, ask: Ask) -> Result<KeyedRequest> {
        let mut state = self.inner.state.lock().await;
        if let Some(made) = state.keyed.iter().find(|r| r.key == ask.key) {
            return if made.base == ask.atomic { Ok(made.clone()) } else { Err(FobError::Screen(ScreenError::RequestKeyTaken)) };
        }
        let account = state.receive_account;
        if account == 0 {
            return Err(FobError::Screen(ScreenError::NoReceiveAccount));
        }
        if ask.atomic == 0 {
            return Err(FobError::Screen(ScreenError::RequestNeedsAmount));
        }
        let tag_id = state.tag_id.get_or_insert_with(|| {
            let mut bytes = [0u8; 16];
            OsRng.fill_bytes(&mut bytes);
            hex::encode(bytes)
        });
        let tag = tag_for(tag_id, &ask.key);
        let now = now();
        let minor = pick_minor(&state.keyed, account, now);
        let id = state.keyed_next_id.max(state.keyed.iter().map(|r| r.id + 1).max().unwrap_or(1)).max(1);
        let request = KeyedRequest {
            id,
            key: ask.key,
            account,
            minor,
            address: self.inner.keys.address_at(account, minor),
            base: ask.atomic,
            tag,
            atomic: ask.atomic.saturating_add(tag),
            created: now,
            expires_at: ask.expires_at,
            height: state.tip,
            cancelled: None,
            note: ask.note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()),
        };
        state.keyed.push(request.clone());
        state.keyed_next_id = id + 1;
        if let Err(e) = self.save_state(&state) {
            state.keyed.pop();
            return Err(e);
        }
        drop(state);
        // A pool that grew past what's watched is watched from the next scan.
        self.inner.wake.notify_one();
        Ok(request)
    }

    /// The keyed request `id`.
    pub async fn keyed_request(&self, id: u64) -> Option<KeyedRequest> {
        self.inner.state.lock().await.keyed.iter().find(|r| r.id == id).cloned()
    }

    /// The keyed request made with `key`.
    pub async fn keyed_request_by_key(&self, key: &str) -> Option<KeyedRequest> {
        self.inner.state.lock().await.keyed.iter().find(|r| r.key == key).cloned()
    }

    /// Every keyed request, oldest first.
    pub async fn keyed_requests(&self) -> Vec<KeyedRequest> {
        self.inner.state.lock().await.keyed.clone()
    }

    /// Calls a keyed request off now: it's no longer open, and its address
    /// is free again after the late window. Calling off one already ended
    /// changes nothing.
    pub async fn cancel_request(&self, id: u64) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let now = now();
        let request = state.keyed.iter_mut().find(|r| r.id == id).ok_or(FobError::Screen(ScreenError::NoSuchRequest))?;
        if request.cancelled.is_some() || request.ends() <= now {
            return Ok(());
        }
        request.cancelled = Some(now);
        self.save_state(&state)
    }

    /// Every output seen at an address of a keyed request: in blocks
    /// scanned, in the pool, and those that left the pool unmined.
    fn keyed_seen(&self, state: &State) -> Vec<Seen> {
        let watched: HashSet<(u32, u32)> = state.keyed.iter().map(|r| (r.account, r.minor)).collect();
        let mut seen: Vec<Seen> = state
            .outputs
            .iter()
            .filter(|o| watched.contains(&(o.account, o.subaddress)))
            .map(|o| Seen { tx: o.tx.clone(), account: o.account, minor: o.subaddress, atomic: o.amount, height: Some(o.height), time: o.time, double_spent: false })
            .collect();
        let mined: HashSet<String> = seen.iter().map(|s| s.tx.clone()).collect();
        let pool = self.inner.pool.lock().expect("pool lock");
        let mut remembered = self.inner.keyed_pool.lock().expect("keyed pool lock");
        // Those mined are no longer the pool's to remember.
        remembered.retain(|tx, _| !mined.contains(tx));
        for (tx, pooled) in pool.iter().filter(|(tx, _)| !mined.contains(*tx)) {
            let parts = pooled.parts.iter().map(|&(minor, atomic)| (0, minor, atomic)).chain(pooled.accounts.iter().copied());
            for (account, minor, atomic) in parts.filter(|(a, m, _)| watched.contains(&(*a, *m))) {
                let entry = remembered.entry(tx.clone()).or_default();
                let s = Seen { tx: tx.clone(), account, minor, atomic, height: None, time: pooled.seen, double_spent: false };
                match entry.iter_mut().find(|r| r.seen.account == account && r.seen.minor == minor) {
                    Some(r) => {
                        r.seen = s;
                        r.left_at = None;
                    }
                    None => entry.push(Remembered { seen: s, left_at: None }),
                }
            }
        }
        for (tx, kept) in remembered.iter_mut() {
            let in_pool = pool.contains_key(tx);
            for r in kept.iter_mut() {
                if in_pool {
                    seen.push(r.seen.clone());
                    continue;
                }
                let left_at = *r.left_at.get_or_insert(state.tip);
                let double_spent = state.tip.saturating_sub(left_at) >= DOUBLE_SPEND_BLOCKS;
                seen.push(Seen { double_spent, ..r.seen.clone() });
            }
        }
        seen
    }

    /// The keyed requests' payments, matched, under the state lock.
    fn keyed_matching(&self, state: &State) -> Matching {
        match_outputs(&state.keyed, &self.keyed_seen(state))
    }

    fn incoming(state: &State, (seen, exact): &(Seen, bool)) -> Incoming {
        Incoming {
            tx: seen.tx.clone(),
            atomic: seen.atomic,
            height: seen.height,
            confirmations: seen.height.map_or(0, |h| confirmations(state, h)),
            double_spent: seen.double_spent,
            exact: *exact,
        }
    }

    /// Every payment toward keyed request `id`, each in full, oldest first:
    /// mined, in the pool, or double spent.
    pub async fn request_payments(&self, id: u64) -> Result<Vec<Incoming>> {
        let state = self.inner.state.lock().await;
        if !state.keyed.iter().any(|r| r.id == id) {
            return Err(FobError::Screen(ScreenError::NoSuchRequest));
        }
        let matching = self.keyed_matching(&state);
        let mut paid: Vec<Incoming> = matching.paid.get(&id).map(|all| all.iter().map(|m| Self::incoming(&state, m)).collect()).unwrap_or_default();
        paid.sort_by_key(|p| (p.height.is_none(), p.height));
        Ok(paid)
    }

    /// Outputs to keyed requests' addresses that count toward none: shown
    /// as "a payment of an unexpected amount", and, with a foreign tag, the
    /// sign another wallet uses the same account.
    pub async fn unexpected_payments(&self) -> Vec<Unexpected> {
        let state = self.inner.state.lock().await;
        self.keyed_matching(&state)
            .unexpected
            .iter()
            .map(|(s, foreign)| Unexpected {
                tx: s.tx.clone(),
                account: s.account,
                minor: s.minor,
                atomic: s.atomic,
                height: s.height,
                confirmations: s.height.map_or(0, |h| confirmations(&state, h)),
                foreign_tag: *foreign,
            })
            .collect()
    }

    /// Tells the listener which keyed requests' payments changed since it
    /// was last told: after a scan, a look at the pool, or a new block
    /// (confirmations count too).
    pub(super) async fn tell_requests(&self) {
        let changed = {
            let state = self.inner.state.lock().await;
            if state.keyed.is_empty() {
                return;
            }
            let matching = self.keyed_matching(&state);
            let digest: HashMap<u64, Vec<Incoming>> = state
                .keyed
                .iter()
                .map(|r| (r.id, matching.paid.get(&r.id).map(|all| all.iter().map(|m| Self::incoming(&state, m)).collect()).unwrap_or_default()))
                .collect();
            let mut told = self.inner.keyed_told.lock().expect("keyed told lock");
            let mut changed: Vec<u64> = digest.iter().filter(|(id, paid)| told.get(id) != Some(paid)).map(|(id, _)| *id).collect();
            changed.sort_unstable();
            *told = digest;
            changed
        };
        if changed.is_empty() {
            return;
        }
        let listener = self.inner.listener.lock().expect("listener lock").clone();
        if let Some(listener) = listener {
            listener.on_requests(changed);
        }
    }

    /// Who hears this wallet's changes, without starting the sync loop: for
    /// an app that syncs only when it asks ([`catch_up`](Wallet::catch_up),
    /// [`look_at_pool`](Wallet::look_at_pool)).
    pub fn listen(&self, listener: Arc<dyn SyncListener>) {
        *self.inner.listener.lock().expect("listener lock") = Some(listener);
    }

    /// Manual Sync as a library: scans from the last block scanned to the
    /// newest (`FromLast`), then looks at the pool, and tells the listener
    /// which keyed requests changed. Runs here, not in the sync loop, which
    /// needn't be started.
    pub async fn catch_up(&self) -> Result<()> {
        let _one = self.inner.scan_running.lock().await;
        while self.sync_step().await? {}
        self.pool_now().await?;
        self.tell_requests().await;
        Ok(())
    }

    /// Looks at the pool now (through a light server, asks it), and tells
    /// the listener which keyed requests changed.
    pub async fn look_at_pool(&self) -> Result<()> {
        self.pool_now().await?;
        self.tell_requests().await;
        Ok(())
    }

    async fn pool_now(&self) -> Result<()> {
        match self.backend().await {
            crate::light::Backend::Node => {
                let (_, rpc, _, _) = self.connection().await?;
                self.scan_pool(&rpc).await
            }
            // A light server's poll lists the pool with the rest.
            crate::light::Backend::Light { .. } => self.light_step().await.map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: u64, minor: u32, atomic: u64, created: u64, expires_at: u64) -> KeyedRequest {
        KeyedRequest {
            id,
            key: format!("k{id}"),
            account: 3,
            minor,
            address: format!("a{minor}"),
            base: atomic - atomic % TAG_MODULUS,
            tag: atomic % TAG_MODULUS,
            atomic,
            created,
            expires_at,
            height: 0,
            cancelled: None,
            note: None,
        }
    }

    fn seen(tx: &str, minor: u32, atomic: u64, time: u64) -> Seen {
        Seen { tx: tx.into(), account: 3, minor, atomic, height: Some(10), time, double_spent: false }
    }

    #[test]
    fn a_tag_is_under_a_millionth_and_follows_the_id_and_key() {
        let a = tag_for("till-a", "sale-1");
        assert!(a < TAG_MODULUS);
        assert_eq!(a, tag_for("till-a", "sale-1"));
        assert_ne!(a, tag_for("till-b", "sale-1"));
        assert_ne!(a, tag_for("till-a", "sale-2"));
        // The id's length is part of it: ("ab", "c") isn't ("a", "bc").
        assert_ne!(tag_for("ab", "c"), tag_for("a", "bc"));
    }

    #[test]
    fn the_pool_takes_fresh_addresses_first_then_the_longest_free() {
        let day = LATE_WINDOW;
        assert_eq!(pick_minor(&[], 3, 0), 1);
        // Each new request takes the next fresh one while any is left.
        let mut made: Vec<KeyedRequest> = (1..=POOL_SIZE).map(|i| request(i.into(), i, 1_000_000_000_001, 0, 100)).collect();
        assert_eq!(pick_minor(&made[..10], 3, 50), 11);
        // All 50 held: one more index.
        assert_eq!(pick_minor(&made, 3, 50), POOL_SIZE + 1);
        // Another account's requests hold nothing here.
        assert_eq!(pick_minor(&made, 4, 50), 1);
        // A day after they expired all are free: the one that ended first.
        made[7].expires_at = 10;
        assert_eq!(pick_minor(&made, 3, 100 + day), 8);
        // Cancelled ends it early.
        made[20].cancelled = Some(5);
        assert_eq!(pick_minor(&made, 3, 5 + day), 21);
        // Until the late window passes, an ended request still holds its address.
        assert_eq!(pick_minor(&made, 3, 4 + day), POOL_SIZE + 1);
        // The grown pool is reused too.
        made.push(request(51, POOL_SIZE + 1, 7, 0, 1));
        assert_eq!(pick_minor(&made, 3, 50), POOL_SIZE + 2);
        assert_eq!(pick_minor(&made, 3, 2 + day), POOL_SIZE + 1);
    }

    #[test]
    fn an_exact_amount_matches_its_own_request_even_on_a_reused_address() {
        let old = request(1, 5, 2_000_000_123_456, 0, 100);
        let new = request(2, 5, 3_000_000_654_321, 200 + LATE_WINDOW, 300 + LATE_WINDOW);
        // A late payment of the old sale, after the new one took the address.
        let late = seen("late", 5, 2_000_000_123_456, 250 + LATE_WINDOW);
        let paid = seen("paid", 5, 3_000_000_654_321, 260 + LATE_WINDOW);
        let m = match_outputs(&[old, new], &[late.clone(), paid.clone()]);
        assert_eq!(m.paid[&1], vec![(late, true)]);
        assert_eq!(m.paid[&2], vec![(paid, true)]);
        assert!(m.unexpected.is_empty());
    }

    #[test]
    fn less_than_asked_is_a_partial_payment_only_with_one_request_open() {
        let r = request(1, 5, 2_000_000_123_456, 0, 100);
        let part = seen("part", 5, 1_000_000_000_000, 50);
        assert_eq!(match_outputs(std::slice::from_ref(&r), std::slice::from_ref(&part)).paid[&1], vec![(part.clone(), false)]);
        // More than asked, or after it closed, isn't.
        let over = seen("over", 5, 3_000_000_000_000, 50);
        let after = seen("after", 5, 1_000_000_000_000, 150);
        let m = match_outputs(std::slice::from_ref(&r), &[over.clone(), after.clone()]);
        assert!(m.paid.is_empty());
        assert_eq!(m.unexpected, vec![(over, false), (after, false)]);
        // Two open on one address: no telling which, so neither.
        let other = request(2, 5, 4_000_000_222_222, 10, 100);
        let m = match_outputs(&[r, other], std::slice::from_ref(&part));
        assert!(m.paid.is_empty() && m.unexpected.len() == 1);
    }

    #[test]
    fn a_foreign_tag_is_reported() {
        let r = request(1, 5, 2_000_000_123_456, 0, 100);
        let theirs = seen("theirs", 5, 5_000_000_777_777, 150);
        let round = seen("round", 5, 5_000_000_000_000, 150);
        let ours_elsewhere = seen("ours", 5, 9_000_000_123_456, 150);
        let m = match_outputs(&[r], &[theirs.clone(), round.clone(), ours_elsewhere.clone()]);
        assert_eq!(m.unexpected, vec![(theirs, true), (round, false), (ours_elsewhere, false)]);
    }

    #[test]
    fn outputs_of_one_transaction_to_one_address_count_as_one() {
        let r = request(1, 5, 2_000_000_123_456, 0, 100);
        let halves = [seen("t", 5, 1_000_000_000_000, 50), seen("t", 5, 1_000_000_123_456, 50)];
        let m = match_outputs(&[r], &halves);
        assert_eq!(m.paid[&1].len(), 1);
        assert!(m.paid[&1][0].1, "exact once joined");
        // Outputs elsewhere are none of a keyed request's business.
        assert_eq!(match_outputs(&[], &halves), Matching::default());
    }

    #[test]
    fn the_plan_watches_the_receive_pool_and_each_account() {
        let watch = |pairs: &[(u32, u32)]| Watch(pairs.iter().copied().collect());
        let a = watch(&[(0, 200), (3, 60)]);
        assert!(a.covers(&watch(&[(0, 100)])));
        assert!(!a.covers(&watch(&[(0, 100), (4, 1)])));
        assert!(!a.covers(&watch(&[(3, 61)])));
        let new: Vec<_> = watch(&[(0, 202), (3, 61), (5, 1)]).beyond(&a).map(|s| (s.account(), s.address())).collect();
        assert_eq!(new, vec![(0, 201), (0, 202), (3, 61), (5, 0), (5, 1)]);
        assert_eq!(a.upto(3), Some(60));
    }
}
