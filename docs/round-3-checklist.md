# Round 3 device checklist

Added to by each step of `round-3-plan.md`, walked through on an emulator once
every step is in. Each item: what to do, what should happen.


## Step 1 · Home rework

- [ ] Open Home with all money ready → one balance line (big XMR + "XMR" left, money at the right end); no ready card under it.
- [ ] Settings › Show amounts in › money, back to Home → money large on the left, "2.4813 XMR" at the right end.
- [ ] Tap the balance → the two swap; tap again → back. With prices off → the balance isn't tappable and shows XMR only.
- [ ] A large balance (or a long currency figure) on a narrow phone → the big figure shrinks to fit on one line; nothing wraps or clips.
- [ ] Receive money (not yet 10 confirmations) → card "<ready> ready · <not ready> not yet", the time, a chevron, a thin bar: ink part, track part with green fill.
- [ ] Tap the card → opens smoothly: Ready to spend / Not ready · N coins / All of it with XMR and money, then "All ready about HH:MM · in N min"; tap again → folds.
- [ ] Wait a block → the green fill grows and the times count down (refreshes every 30 s).
- [ ] Nothing ready yet (a fresh wallet's first payment) → bar is all track and green, "0 ready · …".
- [ ] TalkBack on the card → reads one sentence ("… XMR ready to spend, … XMR not yet, all ready about …") and expanded/collapsed.
- [ ] Buttons: Receive outlined, Send orange; no Scan to pay; Activity's header has no Split up.
- [ ] View-only wallet → only Receive, with the watch-only line under it.
- [ ] While catching up → "Balance so far" above the balance, the sync bar and line under the card.
- [ ] Dark mode → the bar's ink part is light, the card on the dark card colour, text readable.

## Step 2 · Coins screen

- [ ] Home › Activity header → a sunken "Coins" pill with the two-coins icon at the right; tapping anywhere near it (48 dp) opens Coins.
- [ ] Coins with money ready and not → intro "Your … XMR is held as N coins. …", the thin bar, "Ready · N coins" card (ink dot, amount, source, money) and "Not ready · N coins" card (green ring, amount, source, ready time, "n of 10").
- [ ] Ready coins are largest first; not-ready coins soonest first; "all by HH:MM" matches Home's time.
- [ ] Sources: today's payment "Received · 08:40"; an older one "Received · 22 Oct"; a split's coins "Split up · 08:31"; a payment's change "Change · <shop or 86Ux … 7kQa>".
- [ ] Split up something, then open Coins while it's unmined → its coins listed one by one as "Split up · HH:MM", not ready, ring empty.
- [ ] Bottom: orange "Split up" with the coin-splitting icon → opens the Split up screen; back returns to Coins.
- [ ] View-only wallet → Coins opens, no Split up button.
- [ ] New wallet with nothing → "No coins yet. Each payment to this wallet arrives as a coin.", no bar, no cards.
- [ ] All ready → no Not ready card; bar all ink.
- [ ] Leave Coins open across a block → rings and "n of 10" move on within 30 s.
- [ ] Prices off → no money on coins or in "Ready · N coins" sum.
- [ ] TalkBack → each coin row read as one item; ring reads "n of 10 confirmations"; card titles are headings.
- [ ] Dark mode → dots and bar readable on dark cards; the pill sunken, not black.
- [ ] Long change name (a long shop name) → source ellipsised on one line.

## Step 3 · Currency toggle B with ɱ

- [ ] Receive with prices on → amount field: "$" in front, "Amount (optional)" placeholder, orange "USD ⇅" chip at the right, "1 XMR = $…" under it.
- [ ] Type 20 → line "≈ 0.1204 XMR · 1 XMR = $…"; the code asks for that XMR (expanded code shows "$20.00" and "0.1204 XMR").
- [ ] Tap the chip → arrows turn half a turn (quick, smooth), sign becomes ɱ, chip "XMR", value converts to 0.1204, line "≈ $20.00 · …". Tap again → back to dollars, arrows keep turning the same way.
- [ ] ɱ looks like the same weight and size as "$" (system font, bold) and sits on the same baseline as the digits.
- [ ] Type "abc" or "20.123" in dollars → line "That isn't an amount"; the code shows the address only.
- [ ] Currency EUR → sign "€", chip "EUR", TalkBack "Amount in euros".
- [ ] Prices off (or no price yet) → sign ɱ, no chip, nothing under the field; typing XMR works; a bad value says "That isn't an amount".
- [ ] Turn prices off while Receive shows a money amount → field drops to XMR and clears.
- [ ] TalkBack: field reads "Amount in US dollars"; chip reads "Amount in US dollars. Switch to XMR", is a button, ≥ 48 dp to touch.
- [ ] Decimal keyboard opens; a comma works as the point.
- [ ] Dark mode → field on the card colour with the outline, typed text light, chip orange with dark text.
- [ ] Settings › Show amounts in → still the segmented control.
- [ ] Send's form → the same field (step 4 moved it there; see below).

## Step 4 · Send B

Camera and permission
- [ ] Fresh install, Home › Send → Android asks for the camera; allow → full-screen camera, top bar (✕, "Send", torch), large viewfinder, "Point at a payment code", folded sheet below.
- [ ] Deny the camera → dark screen, "Allow the camera" centred between the top bar and the sheet (not under the sheet); Paste and the paid-before rows still work; tapping Allow asks again (after "Don't ask again" nothing happens, as before).
- [ ] Allow it from Android's settings, come back to Send → the camera shows.
- [ ] Torch → light on, the button's circle filled and the lamp orange; tap again → off; close Send with it on → light goes off. TalkBack: "Torch, switch, on/off".
- [ ] A phone or emulator without a flash → no torch button.

Safe areas
- [ ] Phone with a punch-hole or notch → ✕, "Send" and the torch sit fully below the status bar and clear of the cutout; nothing clipped; the camera still fills behind the status bar.
- [ ] Gesture navigation and 3-button navigation → the folded sheet's ready line and the pulled-up Review button sit above the navigation bar; the sheet's paper reaches the bottom edge.
- [ ] Landscape (rotate on Send) → top bar clear of the cutout on the side; folded sheet leaves a strip of camera at the top, scrolls if it must; pulled up, no small frame, the form scrolls, Review reachable. (Rotating restarts at Home, as every screen does today: Nav lives in the activity.)
- [ ] Restore › Scan a wallet code → title and back clear of the status bar and cutout too.

Folded sheet
- [ ] "Paste an address" with an address copied → To filled (pattern + "84Bv9 … 99fQ2"), sheet pulls up. Android shows its paste notice only after the tap, never on opening Send.
- [ ] Paste with an empty clipboard → "Nothing to paste." over the camera.
- [ ] Paste with text that isn't an address ("hello") → Rust's reason over the camera; with a mainnet address on stagenet → the wrong-network message.
- [ ] Paid before (after sending to 2–3 addresses) → "PAID BEFORE", up to 3 rows, newest first, each address once: pattern, shop name (receipt) or "86Ux … 7kQa", line "Today · $24.50" / "Lunch money · 27 Oct" / "3 Oct · 0.3 XMR". No split rows. Tap one → To filled, sheet up.
- [ ] New wallet that never sent → no PAID BEFORE section, just Paste and the ready line.
- [ ] Ready line "1.8592 XMR ready to spend · $308.70" matches Home's Ready to spend; prices off → XMR only.
- [ ] Long shop name → ellipsised on one line.

Pulling up and back
- [ ] Drag the handle up → sheet rises smoothly to under a small live frame (camera still moving in it); drag down → folds. Tap the handle → toggles. TalkBack on the handle: "Type the payment instead" / "Back to the camera", collapsed/expanded.
- [ ] Pulled up, Back → folds; Back again → Home. ✕ → Home from either.
- [ ] Pulled up, point the small frame at a payment code → the form fills (or confirm opens for one with an amount).

Scanning
- [ ] Plain address QR → To filled, sheet up, amount empty in money ($ sign, USD chip), Review disabled until an amount.
- [ ] `monero:` code with tx_description, no amount → To filled, Note prefilled with the description, sheet up.
- [ ] `monero:` code with tx_amount → straight to Confirm with that XMR; Back from Confirm → the form pulled up and filled (see Back from Confirm below).
- [ ] Code with an ORF receipt (and amount) → straight to Confirm with the shop's title and the receipt card, as before; after sending, the receipt is on the payment.
- [ ] Code with an amount more than is ready → form opens filled (amount in XMR), the "not enough" reason above Review.
- [ ] Wallet code → "That's a wallet code, not a payment code."
- [ ] Holding a code in view doesn't keep re-filling or re-opening confirm.
- [ ] A `monero:` link from another app → Send opens and reads it as a scan.

Form
- [ ] To: tap the short address → the whole address editable; type a wrong character → pattern disappears, Review disabled; Paste button in the field replaces it from the clipboard.
- [ ] Amount in USD: type 20 → "≈ 0.1204 XMR · 1 XMR = $…"; Review → Confirm shows that XMR (and ≈ $20.00 worth now).
- [ ] Amount in XMR (tap the chip): 0.05 → Confirm shows 0.05 XMR. "abc" or "0" → "That isn't an amount" / Review disabled.
- [ ] Note "Lunch money" → shown on Confirm under the amount; after sending, the payment's screen shows "Today 12:30 · Lunch money"; PAID BEFORE line becomes "Lunch money · Today".
- [ ] Keyboard up on Amount or Note → the form scrolls so the field stays visible; Review not under the keyboard.
- [ ] View-only wallet via a link → Review gives Rust's "can't spend" reason.

Dark mode
- [ ] Sheet on the dark paper, fields on the dark card colour with the outline, text light, handle visible; Paste chip and icon circle sunken, not black; camera overlay text still light; Review orange with dark text.

## Step 5 · Contacts, data

Rust only; checked through the screens of step 6.
- [ ] Add a contact, close Fob, open it again → the contact, its note, photo and send address are still there.
- [ ] Pay a contact's send address → Activity's row is titled with the contact's name; the payment's screen says "To  Sam".
- [ ] Delete that contact → the payment is still in Activity, titled "86Ux … 7kQa" again, and its address is under "Paid, not saved".
- [ ] A wallet made before this build opens with no contacts and nothing else changed.

## Back from Confirm

- [ ] Send, type To, an amount in USD and a note, Review → Confirm; Back (arrow or system) → Send with the sheet pulled up, To, the amount (still in USD) and the note as typed; Review again works.
- [ ] Scan a code with an amount (and a receipt) → Confirm; Back → the form filled with the code's address, amount in XMR and description; holding the same code in view doesn't jump back to Confirm; Review keeps the receipt.
- [ ] Swipe to send → Paid; system Back from Paid → Home, never the filled form.

## Step 6 · Contacts, screens

Getting there
- [ ] Settings → "Contacts" row at the top of Wallet → Contacts. Send's sheet (after a contact exists) → "All" → Contacts. Back returns to where you came from.
- [ ] No contacts, never paid anyone → Contacts shows "No contacts yet. Tap + to add someone you pay.", no search, no "Paid, not saved".

Contacts list
- [ ] After paying 2 addresses → "Paid, not saved" lists both, newest first, pattern + "84Bv … 9fQ2" + "Paid 25 Oct · $33.21"; Save → New contact with that send address shown (pattern + "84Bv9 … 2kLmQ … 99fQ2").
- [ ] Saved contacts in name order (any case); lines "Lunch money · paid 27 Oct", "Paid today", the note, "Not paid yet".
- [ ] Search "lunch" → contacts with it in name or note; "zzz" → "No contact has that in its name or note."
- [ ] A 40-character name → one line, ellipsised, chevron still visible.
- [ ] + (TalkBack: "Add a contact") → New contact.

New / edit card
- [ ] New contact: title "New contact", ✕ (TalkBack "Close"), Save pill; photo circle with "?" and the camera badge on its corner; hint "Add a photo"; send address "None yet, optional" with Scan and Paste; hint "Scan their code or paste it. Leave it empty if they only pay you."
- [ ] Save with no name → "A contact needs a name."; 41 characters → "A name can be up to 40 characters."; note over 80 → its line.
- [ ] Typing a name updates the initial in the circle.
- [ ] Paste an address of the other network → "the wallet code is for mainnet…" style reason; paste a `monero:` code → its address is used.
- [ ] Paste over an existing address → sheet "Found an address", warning "This replaces Sam's current send address (86Uxm … e7kQa). Check the pattern with Sam.", Cancel / Use it.
- [ ] Save → the new contact's card opens; Back → Contacts with it listed.
- [ ] Edit (from the card) → title "Edit contact", fields filled, hint "Scan or paste to change it. Payments already sent stay with Sam.", "Delete contact" red outline at the bottom.
- [ ] Delete contact → sheet "Delete Sam?", the body text, red Delete (white words; dark red words in dark mode), Keep. Keep → closes. Delete → back to Contacts, Sam gone; their payments still in Activity, titled by address, and in "Paid, not saved".
- [ ] Tapping the scrim or Back closes the sheets.

Photo
- [ ] Tap the circle → sheet "Photo": Take a photo, Choose from your photos (Remove photo only once there's one), the footnote.
- [ ] Take a photo, first time → camera permission asked (if not given before), then the system camera; the photo appears cropped to a circle.
- [ ] Choose from your photos → Android's Photo Picker (no storage permission asked); a tall or wide photo is cropped to its centre; a phone photo turned sideways in its EXIF shows upright (Android 9+).
- [ ] A detailed, colourful photo still saves (made smaller until it fits); close Fob and reopen → photo still there.
- [ ] Remove photo → the initial again.

Scanning the send address
- [ ] Scan from the edit card → full-screen camera, ✕ and "Sam's send address" (or "Their send address" for a new unnamed contact) below the status bar, not under a cutout; the hint under the frame.
- [ ] Point at a Receive code → sheet "Found an address" with its pattern and "87Wq2 … Lm4Zx … 2bNcT"; replacing → the warning naming the old one; same address → "That's already Sam's send address."
- [ ] Scan again → camera again; Use it → back on the edit card with the address.
- [ ] A wallet code or a QR that isn't an address → the reason under the hint, nothing found.
- [ ] Landscape → frame shrinks, nothing under the status bar or gesture bar; the sheet's buttons above the gesture bar.

Contact card
- [ ] Avatar 56, name, note; orange Send, outline Request; "Send address · you pay here" with pattern, three-part address and Change; "Also paid at" only when there is one; "Paid to Sam" with each payment (note, day, −XMR, money) newest first; "Nothing paid to Sam yet." when none.
- [ ] Send → Send pulled up with To filled (short address and pattern), amount empty in money. Contact without a send address: label "No send address yet", button "Add", Send opens Send's camera.
- [ ] Request → Receive (requests come in step 8).
- [ ] Change → scanner titled "Sam's send address"; Use it → back on the card with the new address.
- [ ] A payment row → its own screen (receipt screen for a receipt).

Send's sheet
- [ ] With contacts → "CONTACTS" and "All" (orange); up to 3 with a send address, most recently paid first, avatar 30 + name + last payment line ("Lunch · 27 Oct", "Today · $24.50") or note / "Not paid yet". Tap → To filled with their send address, sheet up.
- [ ] Contacts but none with a send address → "Give a contact a send address to pay them from here."
- [ ] No contacts → "PAID BEFORE" as before.

Dark mode
- [ ] Contacts, card, edit card, sheets: cards on the dark card colour, search and chips sunken, Save pill light on dark (ink role), scrim darkens without going black-on-black, avatar initials readable, warning box on the highlight colour, Delete's red readable.

## Step 7 · Saving a contact after paying

Codes to make (Receive on another wallet, or a QR generator): `monero:<addr>?tx_amount=0.01&tx_description=Pizza&recipient_name=Sam` (Sam a contact), the same with `recipient_name=Henry` (nobody), and one with no name.
- [ ] Pay an address with no name in the code, not a contact's → Paid shows an outline "Save as a contact" (person-plus icon); no footnote. Tap → New contact, name empty, note = the payment's note, send address = that address. Save → the card; Back → Paid, button gone.
- [ ] Pay the code named "Sam" (Sam is a contact, different address) → "to a code named “Sam”" under the status; outline button with Sam's avatar "Add to Sam"; "A different Sam? New contact" in orange; the footnote "The name comes from their code, not checked by Fob. …".
- [ ] Add to Sam → Sam's card; "Also paid at · codes named Sam" lists the address, "Not their send address · paid once, today"; the send address unchanged; "Paid to Sam" has the payment. Back → Paid with nothing offered.
- [ ] A different Sam? New contact → New contact named "Sam", note "Pizza", send address empty ("None yet, optional"), and the line "Paid at … from their code: kept with them, not as their send address."
- [ ] Code named "sAM" → still matches Sam.
- [ ] Code named "Henry" (no contact) → "Save as a contact" + footnote; New contact named Henry, note filled, send address empty, the paid-at line. Saved → Henry's card shows it under Also paid at, Send opens Send's camera (no send address).
- [ ] Pay a contact's own send address → nothing offered on Paid.
- [ ] A very long name in the code (60 characters) → cut to 40; the button ellipsised on one line, not wrapping.
- [ ] A name with a line break or a right-to-left override in the code → shown on one line, left to right.
- [ ] Paid, not saved → Save on an address once paid with a code named "Henry" → New contact named Henry with the address as paid at; on an address paid without a name → as the send address.
- [ ] Split up → Paid shows no contact button.
- [ ] Dark mode: outline button and avatar readable, footnote muted, link orange.

## Step 8 · Requests

Receive
- [ ] Open Receive, then Back without pressing anything → no request in Activity; reopen → the same address as before.
- [ ] Type 20 (money) and a note, tap Share → sheet "Who is this for?" with its line, contacts by name with "No requests yet" / "2 requests paid, 1 waiting", "New contact" (+ on a sunken circle), outline "Share without a name". Tapping the scrim or Back closes it, nothing made.
- [ ] Pick Sam → the chip "Sam" (avatar + name) appears over the code, the address and its number change, the system share sheet opens with a `monero:` code for the new address; Activity shows "Request to Sam", "<note> · waiting · <time>", "$20.00 / asked" in grey with the request icon.
- [ ] Share again without changing anything → shares the same request (no second line in Activity). Change the amount → the code goes back to Receive's own address; Share → a new request on yet another address.
- [ ] Copy with no one chosen → a "Request" line "No name · … · waiting"; the clipboard holds the request's address.
- [ ] New address after a request → a number past every request's; never one a request had.
- [ ] Share → New contact → save "Jo" → back on Receive, amount and note as typed, chip "Jo", share sheet opens at once.
- [ ] A 40-character name → chip fills the width with an ellipsis; "Share with …" ellipsised on one line.
- [ ] Tap the chip → the sheet again; Share without a name → chip gone.
- [ ] Many contacts (10+) → the sheet's list scrolls, "Share without a name" stays visible.
- [ ] Sam's card → Request → Receive with "Sam" chosen; Back without sharing → nothing made, nothing on Sam's card.

Activity and the request screen
- [ ] Pay a request's code from another wallet → a separate "From Sam" line, "<note> · not ready · 1 of 10 · <time>" in green; the request's line says "received $…" (in money when asked in money, else XMR). While still in the pool → "From Sam", "<note> · on the way", and the request already "received".
- [ ] Pay it twice → two "From Sam" lines; the request's received is the sum; no "left to pay" anywhere.
- [ ] Tap the request line → "Request": big amount asked, "asked of Sam · <note>", "Today 08:30 · 0.12 XMR at the time", Received "$… · … XMR" (or "Waiting"), "Address N" with three-part address and pattern, "Payments to it" (each opens its payment), "Show the code again" → full-screen white code with the same amount and note, brightness up; close returns.
- [ ] A request with no amount → "Any amount", "for Sam · <note>", right side of its Activity line empty.
- [ ] Delete request → sheet "Delete this request?" with the board's text naming Sam (no name: shorter text), red Delete, Keep. Keep → nothing changes. Delete → back to where it was opened; gone from Activity and Sam's card; earlier payments still "From Sam".
- [ ] Pay the deleted request's address again → arrives as "Received" with the note, no name.
- [ ] Delete Sam (contact) → his requests read "Request" / "No name · …".

Contact card
- [ ] Sam with requests → "Requests · Sam pays you" and "3 addresses"; rows newest first with pattern, note (or "Request"), "Address 9 · today 08:30", "Received $83.03" green or "Waiting" muted, "asked $80.00"; tap → the request.
- [ ] No requests → no Requests section.
- [ ] Something came from Sam → history titled "Payments with Sam", "From Sam" rows ("<note> · Today", +amount green) among the "−" payments to him.

Old wallet and dark mode
- [ ] An existing wallet from before this step opens; Receive shows the address it showed before; Activity unchanged except notes now lead received lines.
- [ ] Dark mode: sheet on paper, rows readable, chip on the card colour, request icon and "asked" muted, "Received" green readable, Delete red readable, the full-screen code white.

## Step 9 · Your profile

The real test needs two devices: one shares, the other scans (an emulator's code shown on a monitor, scanned by a phone, will do).

Onboarding
- [ ] Create a wallet → seed screen → "I've written them down" → "Step 3 of 3 · optional", Skip top right, "Make it yours", photo circle 128 with the camera badge, "Your name", Continue (orange).
- [ ] Skip → Home; Settings shows the person icon and "Set up your profile".
- [ ] Continue with an empty name → "Your profile needs a name." above Continue; nothing saved.
- [ ] Tap the photo → the "Photo" sheet (Take a photo, Choose from your photos; Remove photo only with one); take one, and choose one → the circle shows it; deny the camera → nothing breaks.
- [ ] Type "Henry", Continue → Your profile with the photo and name; Back → Home.
- [ ] Restore a wallet (seed, wallet code, view-only) → "Last step · optional" with the same editor.

Your profile
- [ ] Settings › Wallet: avatar 40, "Your profile", "Henry · name and photo, share as a code", chevron → Your profile.
- [ ] Edit → "Edit your profile" with Back and Save, filled; change the name and remove the photo → Save → back, the new name and initial.
- [ ] A 40-character name → wraps on Your profile, ellipsised on one line on the share screen, two lines at most in Settings.
- [ ] "Name in payment requests" is on at first; off → Receive's line goes and a request's code has no name; on → back.
- [ ] Your shares with none → the line about what a share is. After a few: newest first, "Code shown" (code icon) / "Link copied" / "Link shared" (link icon), "Today 09:15", "Yesterday …", "28 Oct", the pattern on the right, "nothing yet" muted.
- [ ] Pay a share's address from another wallet → "+$…" (or "+… XMR" with prices off) in green on that share.

Share my profile
- [ ] Share my profile → white screen, brightness up, avatar 52 + name + "Scan with Fob to add me", a dense code nearly the screen's width, "A new address for this share, never used again.", Copy link (outline) and Share (orange). No "Or touch phones" strip.
- [ ] Close at once → the share is still listed ("Code shown") and in Activity.
- [ ] Copy link → the clipboard holds `monero:…?recipient_name=Henry&recipient_avatar=…`; Your shares says "Link copied". Share → the system sheet; "Link shared".
- [ ] Each Share my profile shows a different address (the pattern changes); Receive's New address never gives one of them.
- [ ] Dense code at arm's length: another phone reads it from 30–50 cm with the photo at its largest, and on a small (5") screen too. Modules sharp, not blurred at their edges.
- [ ] Without a photo → a much smaller code.
- [ ] Dark mode: the share screen stays white with dark words; Copy link's outline visible.

Receive and requests
- [ ] With a profile and the switch on → under the note, a person icon and "The code names you as “Henry” · change in Settings"; tap → Your profile.
- [ ] Share a request → its code has `recipient_name=Henry`; another Fob paying it shows "to a code named “Henry”".

Activity
- [ ] A share → "Shared your profile", "A new address for this share · 09:15", person icon, no amount; tap → that share's code again (the same address).
- [ ] A payment to a share's address → its own line "Via your profile", "Shared 09:15 · not ready · 1 of 10 · 09:31" in green; its screen says "Address N · via your profile" under From. Shared on an earlier day → "Shared 30 Oct · …".

Scanning a profile (from a second device)
- [ ] Send's camera on another Fob's share code → the sheet: photo 88, name, pattern + "89Hn2 … Tq4Lp … 6wKeR", "A Fob profile. The name and photo are Henry's own, not checked by Fob.", Pay Henry (outline), Add contact (orange). Back → the camera; the same code isn't taken again.
- [ ] Pay Henry → Send's form with To filled; paid → Paid offers "Save as a contact" or "Add to Henry" as for any named code.
- [ ] Add contact (no Henry, address unknown) → Henry's card with the photo and that send address.
- [ ] Contacts › scan icon ("Scan a profile" in TalkBack) → full-screen camera "Scan a profile"; a plain address → "That code has no name, so it isn't a profile."; a request with an amount → "That's a payment code, not a profile."
- [ ] Ask before updating: scan Henry's newer share (a new address) → Add contact → "You have a contact named Henry.", Send address now → new, "Update Henry? Their send address changes to the profile's.", Update Henry / Keep as is / Add as new contact. Keep as is → back to the profile; Henry's card unchanged.
- [ ] Update Henry → his card with the new send address, the note kept; a profile without a photo keeps his photo.
- [ ] A contact "Sam"; a stranger's profile named "sam" → asks, showing Sam → sam, the photo (both avatars), the send address; Add as new contact → two Sams.
- [ ] A profile on an address that's Sam's send address under another name → "This address is Sam's send address."; on one Sam was paid at → "Sam was paid at this address."
- [ ] The same profile again after adding → "Henry already has this profile's name, photo and send address.", Open Henry.
- [ ] A profile link with a broken photo (junk after `recipient_avatar=`, opened as a link) → Send shows the form as for a named code; Contacts' scanner says "The photo in that profile is damaged."
- [ ] Sheets above the gesture bar and inside the sides in landscape; a 40-character name wraps on the sheet, "Pay …" ellipsised on one line.

Old wallet and dark mode
- [ ] A wallet from before this step opens: no profile, Settings "Set up your profile", Activity unchanged.
- [ ] Dark mode: editor, Your profile, the switch card, shares, the scanned sheet and the asking view (orange arrows, "now" muted) readable; avatars' initials readable.

## Step 10 · Touch phones

Emulators have no NFC: on one, the strip must simply not show. Everything else needs two Android phones with NFC, both with a profile, both on the same network. The protocol itself is proved by `swap.rs`'s tests.

One phone, or the emulator
- [ ] Emulator (no NFC) → Share my profile shows no "Or touch phones" strip; the code as large as before.
- [ ] Phone with NFC on → the strip under the code: phone with waves in orange, "Or touch phones", "Both on this screen, backs together: you swap", on a warm grey pill; light and dark mode alike (the screen stays white).
- [ ] NFC off in the phone's settings → the strip says "NFC is off. Tap to turn it on, then come back."; tap → NFC settings; turn it on, Back → the strip says "Both on this screen…".
- [ ] Tap the strip → Touch phones: X top left (TalkBack "Back to the code"), rings round your photo (88), "Touch phones", the board's line, "No NFC? Scan each other's code instead." (tap → the code); Back → the code. Dark mode: rings and words readable on paper.
- [ ] Hold the phone on the share screen to a contactless bank or transit card → nothing taken, maybe "The other phone isn't on Share my profile."; no crash.
- [ ] Open Share my profile and close it without touching → Your shares has only the "Code shown" share; Receive's New address skips only that share's number (the swap uses the share's address, no second one).

Two phones (A and B)
- [ ] Both on Share my profile (the code or Touch phones), backs together a second or two → both buzz and show "Swapped with <other>", both photos with a green tick, "<other> got your profile, and you got <other>'s.", the card with their name, pattern and three-part address, Not now and Add contact.
- [ ] Only A on Share my profile, B on Home → nothing on either; A's Touch phones may say "The other phone isn't on Share my profile."
- [ ] B on Share my profile with its screen locked (power button) → A gets nothing; unlock B onto the share screen → the swap works.
- [ ] B in the background (Home button) with Share my profile under → A gets nothing.
- [ ] Pull the phones apart mid-touch → no Swapped; touching again works.
- [ ] One on stagenet, the other on mainnet → no Swapped; a line saying the address is another network's.
- [ ] Largest profiles (40-character names, photos near 1.5 KB) both ways → swapped in one touch, about a second.
- [ ] Add contact (no clash) → the new contact's card: name, photo, the swapped address as send address; Back → Your profile.
- [ ] Add contact with a contact of that name already → the asking sheet ("You have a contact named Sam.", now → new, Update Sam / Keep as is / Add as new contact) over a scrim; the scrim or Back closes it; Update → Sam's card, updated.
- [ ] Not now → Your profile; Your shares lists "Touched phones with Sam" (phone icon), "Today 09:15", that swap's pattern, "nothing yet"; Activity shows "Shared your profile" for it.
- [ ] Pay the swapped address from the other phone → "Via your profile" in Activity and "+…" on that share.
- [ ] Tap the "Touched phones with Sam" share → its code again; Copy link → still "Touched phones with Sam".
- [ ] Pulled apart just as B buzzes (rare) → B has A, A hasn't B; A's shares may show "Touched phones" with no name; touching again swaps both.
- [ ] A 40-character name → "Swapped with …" wraps; the card's name two lines at most.
- [ ] TalkBack: the strip reads as one button; Swapped's photos are described; a failure line is announced.

## Step 11 · Settings

- [ ] Settings → sections in order: Wallet, Display, Prices, Syncing, Network, Receipts, This phone; no dividers between rows inside a card; scrolls to "Fob 0.1.0".
- [ ] Wallet with a profile → avatar 40 (photo or initial), "Your profile", "Henry · name and photo, share as a code", chevron → Your profile. Without one → person icon, "Set up your profile" → the editor; saved → Your profile.
- [ ] A 40-character name → the profile row's line two lines at most, ellipsised.
- [ ] Wallet → Contacts → Contacts; Back up seed phrase → the words (absent on a view-only wallet); Show wallet code → the wallet code screen.
- [ ] Display → "Theme" with "Same as the phone" and a chevron → a dialog of Same as the phone / Light / Dark, a tick on the current; pick Dark → the app turns dark at once and the row says "Dark".
- [ ] Display → "Currency" with "US dollar" → the currency dialog; pick another → the row and Home follow.
- [ ] "Show amounts in" with "Tap the balance to swap any time" and XMR / USD segmented (orange on the chosen); switch → Home's balance leads with that; tap Home's balance → the segmented control here follows.
- [ ] Prices off → Currency and Show amounts in go; Show prices and Price source stay.
- [ ] Prices → Show prices switch, Price source with its source and how it's asked. Syncing → Node with the address; Instant updates (ZMQ) switch and status.
- [ ] TalkBack: Theme and Currency read title and value as one button; the segmented control is "Show amounts in" with each option a radio button; switches name their rows.
- [ ] Dark mode: cards, muted values, chevrons, the segmented track and orange choice, switches, avatar and person icon all readable; no black.
