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
- [ ] `monero:` code with tx_amount → straight to Confirm with that XMR; Back from Confirm → Home.
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
