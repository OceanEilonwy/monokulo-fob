//! Keys, backup, and which addresses have been handed out.

use super::*;

/// The highest subaddress ever handed out or watched: far past what anyone
/// hands out, and few enough that watching them all is quick, so a bad
/// value (a server's, a damaged file's) can't have scanning register
/// billions or wrap Receive round to addresses already given out.
pub(super) const MOST_ADDRESSES: u32 = 100_000;

impl Wallet {
    pub fn can_spend(&self) -> bool {
        self.inner.keys.can_spend()
    }

    /// Whether the spend key is on a Trezor, which signs and gives key images.
    pub fn is_trezor(&self) -> bool {
        self.inner.keys.device() == Some(Device::Trezor)
    }

    pub fn seed_words(&self) -> Option<(SeedKind, Zeroizing<String>)> {
        self.inner.keys.seed_words()
    }

    /// Home's reminder while a wallet made here hasn't had its seed phrase
    /// written down, with the warning over the words meanwhile; `None` once
    /// it has (or for a restored or view-only wallet).
    pub async fn backup_reminder(&self) -> Option<backup::Reminder> {
        let pending = self.inner.state.lock().await.backup_pending;
        self.inner.keys.seed_word_count().filter(|_| pending).map(backup::reminder)
    }

    /// "I've written them down": no more reminders.
    pub async fn backup_done(&self) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if !state.backup_pending {
            return Ok(());
        }
        state.backup_pending = false;
        if let Err(e) = self.save_state(&state) {
            state.backup_pending = true;
            return Err(e);
        }
        Ok(())
    }

    /// Where a restore elsewhere should start: the wallet code's and Keys'
    /// restore height.
    pub(super) async fn restore_from(&self) -> u64 {
        let state = self.inner.state.lock().await;
        state.restore_height.unwrap_or(state.next_block)
    }

    /// A wallet code for restoring this wallet elsewhere.
    pub async fn wallet_code(&self) -> Zeroizing<String> {
        let keys = &self.inner.keys;
        let height = self.restore_from().await;
        uri::wallet_code(&keys.primary_address(), keys.spend_key_hex().as_deref().map(|s| s.as_str()), &keys.view_key_hex(), height)
    }

    /// Backup and keys' words, and its line in Settings: the seed phrase's
    /// word count and whether it was written down (said only of a wallet
    /// made here); `phone_lock`: the phone has a screen lock set, which
    /// opening it asks for.
    pub async fn backup_hub(&self, phone_lock: bool) -> backup::Hub {
        let state = self.inner.state.lock().await;
        let backed = match () {
            _ if state.restored => backup::Backed::Restored,
            _ if state.backup_pending => backup::Backed::Pending,
            _ => backup::Backed::Up,
        };
        backup::hub(self.inner.keys.seed_word_count(), backed, phone_lock, self.inner.keys.device().map(Device::name))
    }

    /// Keys' page: every key in full, the restore height with its date
    /// when it can be named.
    pub async fn keys_page(&self) -> Vec<backup::KeyGroup> {
        let height = self.restore_from().await;
        let keys = &self.inner.keys;
        backup::page(backup::KeyValues {
            spend: keys.spend_key_hex(),
            view: keys.view_key_hex(),
            address: keys.primary_address(),
            public_view: keys.public_view_key_hex(),
            public_spend: keys.public_spend_key_hex(),
            restore_height: height,
            date: restore_date(self.network(), height),
        })
    }

    pub fn primary_address(&self) -> String {
        self.inner.keys.primary_address()
    }

    /// The next subaddress no one has had: past every one handed out to
    /// Receive or a request, and never the address splits pay to. In a
    /// restored wallet, also [`RESTORE_SKIP`] past the highest the other
    /// phone was paid at, as found so far: the addresses it handed out
    /// after that aren't known here, and aren't given out again. At
    /// [`MOST_ADDRESSES`], the last one again, never one from the start.
    pub(super) fn hand_out(state: &mut State) -> u32 {
        // Receive keeps the one it shows (a file from before requests
        // showed the last one handed out).
        state.receive_own.get_or_insert(state.receive_index);
        if state.restored {
            let theirs = state.outputs.iter().map(|o| o.subaddress).filter(|s| state.fresh_from.is_none_or(|f| *s < f) && Some(*s) != state.split_index);
            if let Some(highest) = theirs.max().filter(|h| *h > 0) {
                state.receive_index = state.receive_index.max(highest.saturating_add(RESTORE_SKIP));
            } else if state.fresh_from.is_none() {
                state.receive_index = state.receive_index.max(RESTORE_SKIP);
            }
        }
        let next = |i: u32| i.saturating_add(1).min(MOST_ADDRESSES);
        state.receive_index = next(state.receive_index);
        if Some(state.receive_index) == state.split_index {
            state.receive_index = next(state.receive_index);
        }
        if state.restored {
            state.fresh_from.get_or_insert(state.receive_index);
        }
        state.receive_index
    }

    /// The highest subaddress scanning watches: [`LOOKAHEAD`] past the
    /// highest handed out or paid, never past [`MOST_ADDRESSES`].
    pub(super) fn watched(receive_index: u32) -> u32 {
        receive_index.saturating_add(LOOKAHEAD).min(MOST_ADDRESSES)
    }

    /// Receive's address, by index: the last one it showed.
    pub(super) fn own_index(state: &State) -> u32 {
        state.receive_own.unwrap_or(state.receive_index)
    }

    /// Whether Receive's address must be a new one before it's shown: none
    /// yet (in a restored wallet, the highest the scan found is the other
    /// phone's), or the last went to a request, a split or a share.
    pub(super) fn must_hand_out(state: &State) -> bool {
        let own = Self::own_index(state);
        own == 0 || state.restored && state.receive_own.is_none() || Some(own) == state.split_index || state.requests.at(own).is_some() || state.profile.at(own).is_some()
    }

    /// Receive's address and its index as it would open now, handing
    /// nothing out and saving nothing: `None` while
    /// [`receive_address`](Wallet::receive_address) would hand out a new
    /// one. A screen reads this before Receive opens; in a restored wallet,
    /// handing one out before the scan finds the other phone's payments
    /// would give out addresses it already did.
    pub async fn shown_receive_address(&self) -> Option<(String, u32)> {
        let state = self.inner.state.lock().await;
        (!Self::must_hand_out(&state)).then(|| {
            let own = Self::own_index(&state);
            (self.inner.keys.address(own), own)
        })
    }

    /// The address to give out now, as Receive opens: the last one it
    /// showed, or a first one; never one a request or splits have.
    pub async fn receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        if Self::must_hand_out(&state) {
            let index = Self::hand_out(&mut state);
            state.receive_own = Some(index);
            self.save_state(&state)?;
        }
        Ok(self.inner.keys.address(Self::own_index(&state)))
    }

    /// A fresh address nobody has seen.
    pub async fn new_receive_address(&self) -> Result<String> {
        let mut state = self.inner.state.lock().await;
        let index = Self::hand_out(&mut state);
        state.receive_own = Some(index);
        self.save_state(&state)?;
        Ok(self.inner.keys.address(index))
    }

    /// Keeps the note put in a payment code for the address at `index`, so
    /// payments to it say what they were for; a blank one forgets it.
    pub async fn remember_receive_note(&self, index: u32, note: &str) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let note = note.trim();
        let changed = if note.is_empty() { state.notes.remove(&index).is_some() } else { state.notes.insert(index, note.to_owned()).as_deref() != Some(note) };
        if changed { self.save_state(&state) } else { Ok(()) }
    }

    /// Receive's address, by index.
    pub async fn receive_index(&self) -> u32 {
        Self::own_index(&*self.inner.state.lock().await)
    }
}
