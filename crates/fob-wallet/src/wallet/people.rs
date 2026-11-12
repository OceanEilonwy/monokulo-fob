//! Contacts, requests, your profile and swapping it by touch.

use super::*;

impl Wallet {
    /// Contacts by name, those whose name or note has `search` in it.
    pub async fn contacts(&self, search: &str, clock: Clock) -> Vec<contacts::Row> {
        let rows = self.activity().await;
        contacts::list(&self.inner.state.lock().await.contacts, &rows, search, clock)
    }

    pub async fn contact(&self, id: u64) -> Option<Contact> {
        self.inner.state.lock().await.contacts.get(id).cloned()
    }

    /// A contact's photo alone, for the row whose `photo_key` names it.
    pub async fn contact_photo(&self, id: u64) -> Option<Vec<u8>> {
        self.inner.state.lock().await.contacts.get(id)?.photo.clone()
    }

    /// A contact's card, or `None` once it's deleted.
    pub async fn contact_card(&self, id: u64, clock: Clock) -> Option<contacts::Card> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        state.contacts.get(id).map(|c| contacts::card(c, &state.requests, &rows, clock))
    }

    /// What the screens list, from one look at the activity: Home's
    /// Activity, by day, with every payment, request and share of your
    /// profile; Contacts and its "Paid, not saved"; Send's sheet's contacts
    /// (with a send address, the most recently paid first) and the people
    /// paid before. Kept, as the activity is, until the wallet changes or
    /// the day does (decision 412).
    pub async fn lists(&self, clock: Clock) -> Arc<Lists> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        // Of the clock, the lists use only which day it is and how times are written.
        let key = ListsKey { rows: rows.clone(), state: state.generation(), today: clock::local(clock.now, clock).0, utc_offset: clock.utc_offset, hours_24: clock.hours_24 };
        if let Some((_, lists)) = self.inner.lists.lock().expect("lists lock").as_ref().filter(|(k, _)| *k == key) {
            return lists.clone();
        }
        let mut lines = requests::activity_rows(&state.requests, &state.contacts, &rows, clock);
        lines.extend(profile::activity_rows(&state.profile, clock));
        lines.sort_by_key(|l| std::cmp::Reverse(l.time));
        let lists = Lists {
            contacts: contacts::list(&state.contacts, &rows, "", clock),
            unsaved: contacts::unsaved(&state.contacts, &rows, clock),
            on_send: contacts::on_send(&state.contacts, &rows, clock),
            paid_before: activity::paid_before(&rows, clock),
            days: vec![],
        };
        drop(state);
        let lists = Arc::new(Lists { days: activity::by_day(rows.to_vec(), lines, clock), ..lists });
        *self.inner.lists.lock().expect("lists lock") = Some((key, lists.clone()));
        lists
    }

    /// The name a request's code carries (`recipient_name`): the profile's,
    /// while "Name in payment requests" is on.
    pub(super) fn name_in_requests(state: &State) -> Option<String> {
        state.profile.name_in_requests().map(Into::into)
    }

    /// Your profile, once it's set up.
    pub async fn profile(&self) -> Option<profile::Profile> {
        self.inner.state.lock().await.profile.profile().cloned()
    }

    /// Settings' row: "Your profile" over "Henry · share as a code", or
    /// only "Add a shareable Fob contact" before there's a name.
    pub async fn profile_row(&self) -> profile::SettingsRow {
        profile::settings_row(self.inner.state.lock().await.profile.profile())
    }

    /// Changes the profile and saves it, or leaves it as it was when
    /// `change` refuses.
    pub(super) async fn change_profile<T>(&self, change: impl FnOnce(&mut profile::Book) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut book = state.profile.clone();
        let done = change(&mut book)?;
        let before = std::mem::replace(&mut state.profile, book);
        if let Err(e) = self.save_state(&state) {
            state.profile = before;
            return Err(e);
        }
        Ok(done)
    }

    /// Sets your name and photo ("Make it yours", or Edit).
    pub async fn set_profile(&self, name: &str, photo: Option<Vec<u8>>) -> Result<profile::Profile> {
        self.change_profile(|book| book.set(name, photo).map_err(Into::into)).await
    }

    /// "Name in payment requests", on or off.
    pub async fn set_name_in_requests(&self, on: bool) -> Result<()> {
        self.change_profile(|book| book.set_name_in_requests(on).map_err(Into::into)).await
    }

    /// "Share my profile": a share on a subaddress no one has had, recorded
    /// now, however briefly its code is shown. Returns it with its code.
    pub async fn share_profile(&self) -> Result<(profile::Share, String)> {
        let mut state = self.inner.state.lock().await;
        let mut changed = state.clone();
        let subaddress = Self::hand_out(&mut changed);
        let share = changed.profile.share(subaddress, self.inner.keys.address(subaddress), profile::ShareKind::CodeShown, now())?;
        self.save_state(&changed)?;
        *state = changed;
        *self.inner.fresh_share.lock().expect("share lock") = Some(share.id);
        let code = profile::code(state.profile.profile(), &share);
        Ok((share, code))
    }

    /// Whether share `id` can still swap by touching phones: the one the
    /// last Share my profile made, its swap not over, its code only shown
    /// (a link that went out, or a touch, already put its address with
    /// someone).
    pub(super) fn can_touch(&self, share: &profile::Share) -> bool {
        *self.inner.fresh_share.lock().expect("share lock") == Some(share.id) && share.kind == profile::ShareKind::CodeShown
    }

    /// Touch phones, while the screen of the share Share my profile just
    /// made is open: a swap of the profile on that share's own address, so
    /// one opening hands out one address however it goes out. Any other
    /// share (reopened from Your shares or Activity, or already gone out
    /// as a link or a touch) is refused: a second person would get an
    /// address someone already has.
    pub async fn start_swap(&self, id: u64) -> Result<Arc<swap::Session>> {
        let state = self.inner.state.lock().await;
        let share = state.profile.get(id).ok_or(FobError::Screen(ScreenError::ShareDeleted))?;
        if !self.can_touch(share) {
            return Err(ScreenError::ShareNotFresh.into());
        }
        let session = Arc::new(swap::Session::new(
            self.network(),
            &profile::swap_code(state.profile.profile(), share),
            share.subaddress,
            share.address.clone(),
        )?);
        *self.inner.live_swap.lock().expect("swap lock") = Arc::downgrade(&session);
        Ok(session)
    }

    /// The swap is over (it worked, or the screen closed): nothing more is
    /// answered or read, and its share swaps no more. The share is kept as
    /// "Touched phones with Sam" when theirs came, or "Touched phones" when
    /// only ours went out whole; unchanged when neither, or when it's kept
    /// so already. Returns the share, their profile as if scanned, and
    /// whether the other phone said it took ours.
    pub async fn end_swap(&self, session: &swap::Session) -> Result<Option<(profile::Share, profile::Scanned, bool)>> {
        let ended = session.end();
        let mut state = self.inner.state.lock().await;
        let Some(id) = state.profile.at(session.subaddress()).map(|s| s.id) else { return Ok(None) };
        {
            let mut fresh = self.inner.fresh_share.lock().expect("share lock");
            if *fresh == Some(id) {
                *fresh = None;
            }
        }
        if ended.got.is_none() && !ended.gave {
            return Ok(None);
        }
        let mut changed = state.clone();
        let name = ended.got.as_ref().map(|p| p.name.clone()).filter(|name| !name.is_empty());
        let Some(share) = changed.profile.touched(id, name) else { return Ok(None) };
        self.save_state(&changed)?;
        *state = changed;
        Ok(ended.got.map(|code| (share, profile::scanned(&state.contacts, code), ended.sure)))
    }

    /// A share's code again (its address, the profile as it is now), and
    /// whether it can still swap by touching phones; `None` for no such
    /// share.
    pub async fn share_code(&self, id: u64) -> Option<(profile::Share, String, bool)> {
        let state = self.inner.state.lock().await;
        let share = state.profile.get(id)?;
        Some((share.clone(), profile::code(state.profile.profile(), share), self.can_touch(share)))
    }

    /// A share's code as Meet's second one, after the other phone's was
    /// scanned (marked as a reply, so scanning it finishes the meeting).
    pub async fn share_reply_code(&self, id: u64) -> Option<String> {
        let state = self.inner.state.lock().await;
        let share = state.profile.get(id)?;
        Some(profile::reply_code(state.profile.profile(), share))
    }

    /// Copy link or Share pressed on a share's screen: kept as that kind.
    /// Its address is now with whoever gets the link, so a swap running on
    /// it ends at once.
    pub async fn share_went_out(&self, id: u64, kind: profile::ShareKind) -> Result<()> {
        let live = self.inner.live_swap.lock().expect("swap lock").upgrade();
        if let Some(session) = live
            && self.inner.state.lock().await.profile.at(session.subaddress()).is_some_and(|s| s.id == id)
        {
            session.end();
        }
        self.change_profile(|book| Ok(book.went_out(id, kind))).await.map(|_| ())
    }

    /// Meet's QR code tab finished with `name`: share `id` is kept as met.
    pub async fn share_met(&self, id: u64, name: &str) -> Result<()> {
        self.change_profile(|book| Ok(book.met(id, name.to_owned()))).await.map(|_| ())
    }

    /// Your shares, newest first, each with what came to its address.
    pub async fn profile_shares(&self, clock: Clock) -> Vec<profile::Line> {
        let rows = self.activity().await;
        profile::lines(&self.inner.state.lock().await.profile, &rows, clock)
    }

    /// A scanned profile code, and the contact it would change, if any.
    pub async fn scanned_profile(&self, text: &str) -> Result<profile::Scanned> {
        let code = uri::parse_profile(self.network(), text)?;
        Ok(profile::scanned(&self.inner.state.lock().await.contacts, code))
    }

    /// "Add contact" on a scanned profile, or "Add as new contact" once
    /// asked (`as_new`): its name, photo and address as the send address.
    /// Checked again here, whatever the screen showed: a profile whose
    /// address is a contact's is never added beside it (Update is the way),
    /// and one whose name is (or looks like) a contact's only once asked.
    /// `met`: it came from Meet (a touch, or both codes), so they have our
    /// address too; otherwise the contact is half set up.
    pub async fn add_contact_from_profile(&self, text: &str, as_new: bool, met: bool) -> Result<Contact> {
        let code = uri::parse_profile(self.network(), text)?;
        self.change_contacts(|book, network, _| {
            if let Some(owner) = book.owner(&code.address) {
                return Err(ScreenError::AddressIsContacts { name: owner.name.clone() }.into());
            }
            if let Some(named) = book.named(&code.name).filter(|_| !as_new) {
                return Err(ScreenError::NameIsContacts { name: named.name.clone() }.into());
            }
            let added = book.add(network, profile::new_contact(&code), now())?;
            if met { Ok(added) } else { Ok(book.set_half(added.id, true)?) }
        })
        .await
    }

    /// "Update Sam" on a scanned profile, once asked: the contact takes its
    /// name, photo and address as the send address.
    /// `met`: it came from Meet, so the contact is no longer half set up.
    pub async fn update_contact_from_profile(&self, id: u64, text: &str, met: bool) -> Result<Contact> {
        let code = uri::parse_profile(self.network(), text)?;
        self.change_contacts(|book, network, paid| {
            let contact = book.get(id).ok_or(FobError::Screen(ScreenError::ContactDeleted))?;
            let edited = book.edit(network, id, profile::updated(contact, &code), paid)?;
            if met { Ok(book.set_half(id, false)?) } else { Ok(edited) }
        })
        .await
    }

    /// Makes a request, when Share or Copy is pressed on Receive (never by
    /// opening it): on a subaddress no one has had, for `contact` (or no
    /// one), asking `amount_text` (in money when `in_fiat`, converted at the
    /// current price as the field's line showed) with `note`, its code
    /// naming the profile while requests carry it. The same ask again, while
    /// nothing has come to the last one, gives the last one again
    /// ([`requests::Book::same_again`]). Returns it with its code.
    pub async fn make_request(&self, contact: Option<u64>, amount_text: Option<&str>, in_fiat: bool, note: Option<&str>) -> Result<(requests::Request, ReceiveCode)> {
        let rows = self.activity().await;
        let typed = amount_text.map(str::trim).filter(|a| !a.is_empty());
        let asked = match typed {
            None => None,
            Some(t) => {
                let (xmr, fiat) = self.typed(t, in_fiat).await?;
                Some(requests::Asked {
                    typed: t.to_owned(),
                    unit: fiat.as_ref().map(|(_, code)| code.clone()).unwrap_or_else(|| "XMR".into()),
                    xmr,
                    fiat: fiat.map(|(text, _)| text),
                })
            }
        };
        let mut state = self.inner.state.lock().await;
        if contact.is_some_and(|id| state.contacts.get(id).is_none()) {
            return Err(FobError::Screen(ScreenError::ContactDeleted));
        }
        let name = Self::name_in_requests(&state);
        let ask = requests::Draft { subaddress: 0, address: String::new(), contact, note: note.map(Into::into), asked, name };
        if let Some(again) = state.requests.same_again(&ask, &rows) {
            return Ok((again.clone(), Self::request_code_of(again)));
        }
        let mut changed = state.clone();
        let subaddress = Self::hand_out(&mut changed);
        let draft = requests::Draft { subaddress, address: self.inner.keys.address(subaddress), ..ask };
        let request = changed.requests.add(draft, now())?;
        self.save_state(&changed)?;
        *state = changed;
        let code = Self::request_code_of(&request);
        Ok((request, code))
    }

    pub(super) fn request_code_of(request: &requests::Request) -> ReceiveCode {
        let note = request.note.clone();
        let (headline, detail) = match &request.asked {
            Some(asked @ requests::Asked { fiat: Some(fiat), .. }) => {
                (Some(fiat.clone()), Some([Some(format!("{} XMR", amount::format(asked.xmr))), note].into_iter().flatten().collect::<Vec<_>>().join(" · ")))
            }
            Some(asked) => (Some(format!("{} XMR", amount::format(asked.xmr))), note),
            None => (None, note),
        };
        ReceiveCode { uri: requests::code_uri(request), headline, detail }
    }

    /// A request's code again, as it was made ("Show the code again"),
    /// whatever the profile's name is now.
    pub async fn request_code(&self, id: u64) -> Option<ReceiveCode> {
        let state = self.inner.state.lock().await;
        Some(Self::request_code_of(state.requests.get(id)?))
    }

    /// A request's own screen, or `None` once it's deleted.
    pub async fn request(&self, id: u64, clock: Clock) -> Option<requests::Detail> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        state.requests.get(id).map(|r| requests::detail(r, &state.contacts, &rows, clock))
    }

    /// Deletes a request: it leaves Activity and its contact's card; its
    /// address stays this wallet's, and what's paid there later shows
    /// without a name.
    pub async fn delete_request(&self, id: u64) -> Result<()> {
        // What's in already, the pool's too, stays named when it's mined.
        let rows = self.activity().await;
        let mut state = self.inner.state.lock().await;
        let mut changed = state.requests.clone();
        if changed.delete(id, now(), &rows) {
            let before = std::mem::replace(&mut state.requests, changed);
            if let Err(e) = self.save_state(&state) {
                state.requests = before;
                return Err(e);
            }
        }
        Ok(())
    }

    /// Share's sheet ("Who is this for?"): every contact, each with its
    /// requests in a line.
    pub async fn request_contacts(&self) -> Vec<contacts::Row> {
        let rows = self.activity().await;
        let state = self.inner.state.lock().await;
        requests::sheet(&state.contacts, &state.requests, &rows)
    }

    /// Changes the contacts and saves them, or leaves them as they were when
    /// `change` refuses. `change` is told which addresses were paid.
    pub(super) async fn change_contacts<T>(&self, change: impl FnOnce(&mut contacts::Book, Network, &dyn Fn(&str) -> bool) -> Result<T>) -> Result<T> {
        let mut state = self.inner.state.lock().await;
        let mut book = state.contacts.clone();
        let paid: HashSet<&str> = state.sent.iter().filter(|s| s.pieces.is_none()).map(|s| s.to.as_str()).collect();
        let done = change(&mut book, self.network(), &|a| paid.contains(a))?;
        state.contacts = book;
        self.save_state(&state)?;
        Ok(done)
    }

    pub async fn add_contact(&self, draft: contacts::Draft) -> Result<Contact> {
        self.change_contacts(|book, network, _| book.add(network, draft, now()).map_err(Into::into)).await
    }

    pub async fn edit_contact(&self, id: u64, draft: contacts::Draft) -> Result<Contact> {
        self.change_contacts(|book, network, paid| book.edit(network, id, draft, paid).map_err(Into::into)).await
    }

    /// A contact's send address, scanned or pasted (a payment code's
    /// address is taken).
    pub async fn set_contact_send_address(&self, id: u64, text: &str) -> Result<Contact> {
        self.change_contacts(|book, network, paid| book.set_send_address(network, id, text, paid).map_err(Into::into)).await
    }

    /// Keeps an address paid (from a code named for the contact) with it.
    pub async fn add_contact_paid_at(&self, id: u64, address: &str) -> Result<Contact> {
        self.change_contacts(|book, network, _| book.add_paid_at(network, id, address).map_err(Into::into)).await
    }

    /// The payment to `address` a contact made from it would be named by:
    /// the newest one from a code with a name (once paid as a named code's,
    /// an address is never offered as a send address), else the newest.
    pub(super) fn naming_payment<'a>(state: &'a State, address: &str) -> Option<&'a Sent> {
        let mut paid: Vec<&Sent> = state.sent.iter().filter(|s| s.to == address && s.pieces.is_none()).collect();
        paid.sort_by_key(|s| std::cmp::Reverse(s.time));
        paid.iter().find(|s| s.recipient_name.is_some()).or(paid.first()).copied()
    }

    /// What the Paid screen offers for the payment `tx` (Add to Sam, or
    /// Save as a contact), or `None`: a split, or an address that's already
    /// a contact's. Its name is this payment's code's, else the one any
    /// earlier payment to the address went under.
    pub async fn save_offer(&self, tx: &str) -> Option<contacts::Offer> {
        let state = self.inner.state.lock().await;
        let sent = state.sent.iter().find(|s| s.tx == tx && s.pieces.is_none())?;
        let name = sent.recipient_name.as_deref().or_else(|| Self::naming_payment(&state, &sent.to)?.recipient_name.as_deref());
        contacts::offer(&state.contacts, &sent.to, name)
    }

    /// New contact for an address paid that's no contact's ("Paid, not
    /// saved"): as the Paid screen would have offered after its last
    /// payment, a code's name making it an address they were paid at.
    pub async fn new_contact_for(&self, address: &str) -> contacts::Draft {
        let state = self.inner.state.lock().await;
        let named = Self::naming_payment(&state, address);
        contacts::new_contact(address, named.and_then(|s| s.recipient_name.as_deref()))
    }

    /// Deletes a contact; payments to its addresses stay.
    pub async fn delete_contact(&self, id: u64) -> Result<()> {
        self.change_contacts(|book, _, _| {
            book.delete(id);
            Ok(())
        })
        .await
    }

    /// Receive's payment code for `address`, with an amount typed in XMR or,
    /// when `in_fiat`, in money (the code still asks in XMR, converted at
    /// the current price), and a note for the payer.
    pub async fn receive_code(&self, address: &str, amount_text: Option<&str>, in_fiat: bool, note: Option<&str>) -> Result<ReceiveCode> {
        let typed = amount_text.map(str::trim).filter(|a| !a.is_empty());
        let (xmr, fiat) = match typed {
            None => (None, None),
            Some(t) => {
                let (xmr, fiat) = self.typed_amount(t, in_fiat).await?;
                (Some(xmr), fiat)
            }
        };
        let xmr = xmr.filter(|a| *a > 0);
        let xmr_text = xmr.map(amount::format);
        let code = uri::receive(address, xmr_text.as_deref(), note)?;
        Ok(match (fiat.filter(|_| xmr.is_some()), xmr_text) {
            (Some(fiat), Some(xmr)) => ReceiveCode {
                uri: code.uri,
                headline: Some(fiat),
                detail: Some([Some(format!("{xmr} XMR")), code.detail].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
            },
            _ => ReceiveCode { uri: code.uri, headline: code.headline, detail: code.detail },
        })
    }
}

/// What the screens' lists were built from: the activity (the same rows,
/// not rows alike), the wallet's requests, contacts and profile as they
/// were, and the clock's day and way of writing times.
pub(super) struct ListsKey {
    rows: Arc<[Activity]>,
    state: u64,
    today: i64,
    utc_offset: i32,
    hours_24: bool,
}

impl PartialEq for ListsKey {
    fn eq(&self, other: &ListsKey) -> bool {
        Arc::ptr_eq(&self.rows, &other.rows) && (self.state, self.today, self.utc_offset, self.hours_24) == (other.state, other.today, other.utc_offset, other.hours_24)
    }
}
