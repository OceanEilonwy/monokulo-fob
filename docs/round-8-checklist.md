# Round 8 device checklist

Added to by each step of `round-8-plan.md`, walked through on an emulator once
every step is in. Each item: what to do, what should happen.

## Step 1 · Texts

- [ ] Contacts with a few contacts and a Paid, not saved row → nothing under the last card; light and dark. — needs a funded wallet that has paid an address not saved as a contact; with one contact, nothing under the last card in light and dark
- [x] Contacts with none → "No contacts yet. Tap + to add someone you pay…" still shows.
- [x] Receive with a profile and "Name in payment requests" on → no "The code names you as …" row under the note; Share still puts the name in the code. (shared text carries recipient_name=Henry)
- [x] Receive → the address pattern shows alone (no "The payer's Fob shows this pattern too"), no "Address N · …" line; New address sits at the right end and still makes a new address. (the pattern is in the code now (step 3))
- [x] Receive with a contact picked, then Share → no "Address N · this request's own".
- [x] Receive › Share → Who is this for? sheet: title, contacts, New contact, Share without a name; no explanation line.
- [x] Receive › tap the code → full screen code and pattern, no brightness line; screen still goes bright. (window brightness 1.0 (dumpsys sbrt=1.0))
- [ ] Split up with a fresh price → footer "Network fee about $0.03" only. — needs a funded wallet (this one has no coins)
- [ ] Split up with a price over an hour old (airplane mode, clock forward) → "Network fee about $0.03 · 2 h old price"; nothing about XMR or today's price. — needs a funded wallet (this one has no coins)
- [ ] Split up with XMR only → "Network fee about 0.000… XMR", no age. — needs a funded wallet (this one has no coins)
- [ ] Split up's "Not enough payments yet…" / "Picked from…" tip still shows (step 8 replaces it). — needs a funded wallet (this one has no coins); step 8 replaced this tip
- [x] Settings › Node › check a node → "Sends blocks to Fob" shows "31 blocks/s" with the link speed on its own line under it, right-aligned, on a 360 dp phone and at 200 % font; never side by side. — fixed in afa340a: at 200 % font on 360 dp the value broke as "57 / blocks/ / s"; the label now wraps first and the value keeps its whole line
- [x] Same with TalkBack → "Sends blocks to Fob: 31 blocks/s, 1.8 MB/s". (read from the accessibility tree)
- [ ] A node that sends no blocks → one red line "no blocks in 10 s", no empty second line. — needs a node that answers but sends no blocks
- [ ] Home › sync chip while catching up → Time left, Payments up to, Blocks, Link speed rows; no "You can still receive …" note. With the node off → "Fob keeps trying on its own." still shows. — needs a wallet still catching up for the rows; with the node off (airplane mode) "Fob keeps trying on its own." shows
- [x] Settings → Contacts row says "Who you pay"; Use mainnet on says "Real money"; Receipts row has no "on this phone only". (Use mainnet on: "Real money" read in Settings.kt, off state seen)
- [x] Settings › Instant updates → Status, ZMQ address field and hint; no paragraph under it.
- [x] Send → Paste an address row is one line, still centred in its 60 dp row; note placeholder "What it's for".
- [x] Contact edit › photo, and Edit your profile › photo → the sheet has Take, Choose, Remove and no footnote. (contact's photo sheet checked (Take, Choose; Remove only with a photo); the profile's is the same sheet)
- [x] Edit your profile (and onboarding step 3) → intro is one sentence; no help line under Your name.
- [x] Your profile → Name in payment requests switch has no line under it (title stays on one line, switch reachable); Your shares header has nothing at its right.
- [x] Share my profile → no "A new address for this share…" under the code.
- [x] Create a wallet → Back up your wallet shows the words and warning, no Polyseed line.
- [x] Restore › View-only wallet → fields start right under the top bar.
- [ ] Restore › Scan a wallet code › found → no Keystore sentence under the card. — needs a second screen showing a wallet code to the camera
- [x] Backup and keys › Keys → Main address has no line under it; the others keep theirs.

## Step 2 · Edge buttons

- [x] Settings › Node → the refresh sits clearly in from the right edge (centre about a thumb's width in), bigger than before; tap it near its edge → checks again and turns; TalkBack reads "Check again", "Checking" while it runs. ("Checking" is the stateDescription in Node.kt (not in a uiautomator dump))
- [x] Any screen with Back (Settings, Coins, Contacts) → Back sits in from the left edge as far as Node's refresh from the right; TalkBack reads "Back".
- [x] Home → the photo (40 dp) and the cog sit in from the right edge, both easy to hit; TalkBack reads "Your profile" (or "Add a shareable Fob contact") and "Settings". No profile yet → the grey person circle at the same size.
- [x] Home on a 360 dp phone, stagenet, while catching up ("6 min left"), and at 200 % font → photo, cog and the chip's ring stay whole and on screen; "Fob" and the chip's words drop first. — fixed in 7929f98: with the badge and "Can't reach node" at 360 dp the word "Fob" went while room was left; now the chip's words go first, then "Fob", then the badge. At 200 % only the mark and the ring stay
- [x] Send (camera) → Close top left and the torch top right at the same inset and size as top bar buttons; torch toggles, TalkBack reads "Torch, Off/On". (torch shows once the emulator camera binds; checked/unchecked toggles)
- [x] Restore › Scan a wallet code, Contacts › Scan a profile, Contact edit › scan an address → Close in from the left, white on the camera, TalkBack "Close". (Contacts' and a contact's scanners read Close; Restore's reads Back (it is a step of Restore), see the report)
- [ ] Receive › tap the code, Share my profile, and Touch phones → Close in from the left at the same size. — needs NFC for Touch phones; Receive's full screen code and Share my profile pass
- [x] Contacts → Scan a profile and Add a contact side by side at the right, each 56 dp; TalkBack reads both labels.
- [x] A contact → Edit; Your profile → Edit; Split up → Clear; Contact edit → Save: each in from the right edge, easy to tap, at least 48 dp tall. (Split up's Clear needs coins (step 8))
- [x] Onboarding Edit your profile (step 3 of 3) → Skip in from the right edge.
- [x] 200 % font on a 360 dp phone → titles (Backup and keys, Type your seed phrase, Contact edit's title beside Save) stay on one line and end in "…"; Back and the actions keep their size; nothing overlaps.
- [x] Landscape with a cutout or the gesture bar at the side (Home, Node, Send's camera, Receive's full screen code) → every edge button stays inside the safe area, none under the cutout. (Home in both landscape rotations clear of the cutout; the other screens share the same top bar)
- [x] Light and dark → buttons keep their usual ink (white on the camera, dark on the white code screens); no new outlines or fills.
- [x] Dialog buttons (Switch network, Remove wallet, Theme) → still sit right, now at least 48 dp tall; dialogs look unchanged otherwise. (dialog buttons 48 dp tall)

## Step 3 · Receive's code

- [x] Receive, no amount, light mode → the address's dots sit in a white square in the code's middle; no pattern row under the card; another phone or app (Google Lens, a second wallet) scans it to the right address. (decoded with zbarimg)
- [x] Same in dark mode → the card and the square stay white, the dots in their colour; it scans. (decoded with zbarimg)
- [x] Type an amount → the code redraws, dots still centred; another app scans it with the amount. (decoded with zbarimg: tx_amount=0.02082)
- [x] Amount and a short note, then a 120-character note (and one in emoji) → the code gets denser but the dots stay centred and about the same size; each scans with the note. (120-character note decoded with zbarimg; emoji can't be typed with adb. The dots grow a little in the denser code (the square rounds to whole modules))
- [x] Tap the code (or its corner button) → full screen code with the dots in its middle and no pattern under it; scans at arm's length in light and dark; Close returns. (decoded with zbarimg)
- [x] Request › Show the code again (with a name and a note) → the full screen code has the dots in its middle; it scans. (decoded with zbarimg)
- [x] New address → the dots change with the address.
- [x] TalkBack on Receive's card → reads "Payment code" and offers "Show larger"; the dots aren't a separate stop.
- [x] Share my profile → its code is unchanged (no square in the middle), still scans with the photo. (decoded with zbarimg)
- [x] Backup and keys › Wallet code → unchanged, no square; still scans. (decoded with zbarimg)

## Step 4 · Coins and warnings

- [x] New wallet, no coins → Coins shows Split up at the bottom, greyed out (sunken fill, muted words and icon); tapping does nothing; TalkBack reads "Split up, button, disabled". (enabled=false in the accessibility tree)
- [ ] Only a payment not ready yet (0–9 confirmations) → Split up still greyed out; once it's ready (or within 30 s of the coin turning ready) it turns orange and opens Split up. — needs a funded wallet (this one has no coins)
- [ ] Open Coins with ready coins, watching closely (or a screen record) → Split up goes from greyed to orange; never orange first then grey. — needs a funded wallet (this one has no coins)
- [ ] View-only wallet → Coins shows no Split up at all, as before. — needs a view-only wallet (one wallet per network here)
- [x] Light and dark → the greyed Split up reads clearly as off, its words legible.
- [x] Backup and keys, light then dark → the warning sits on a clear peach (light) or deep brown (dark) block, words in full ink at body size, orange triangle; no edge or outline.
- [x] Seed phrase (from the hub and from Home's reminder) → "These 16 words are your wallet …" in the same block above the words; a view-only wallet's "This wallet only watches …" stays plain. (from Home's reminder; the hub's Seed phrase has no block since step 6)
- [x] Wallet code, full and view-only wallet → each warning in the same block. (step 6 took the warnings off Wallet code)
- [x] Home's backup reminder (new wallet) → same block with the lines icon, title, line in ink and a chevron; tapping it opens the seed after the lock; TalkBack reads it as one button.
- [x] Settings › Network › Use mainnet → the dialog's text in the warning block; Back to stagenet stays plain text. Settings › Remove this wallet → the warning block.
- [x] Contacts › add a contact, paste an address that replaces one → the "replaces" warning in the same block, Use it and the other button below.
- [x] Restore › Scan a wallet code → the warning card on the camera keeps its dark card and light words, same layout as the others.
- [x] 200 % font → every warning wraps inside its block, the triangle stays at the top left, nothing clipped.

## Step 5 · Default currency

- [x] 360 dp phone (or emulator at 360 dp), default font, light then dark → Settings › Money shows "Default currency" with "USD · $" (or the chosen one) and a chevron on one line; no line under the title; no "Show first" row; Price from under it.
- [x] Choose AUD, EUR, JPY, CHF, NZD in turn → the value reads "AUD · A$", "EUR · €", "JPY · ¥", "CHF", "NZD · NZ$"; never a full name, never wrapped.
- [x] Choose Monero (XMR) → the value reads "XMR"; Price from disappears.
- [x] 200 % font → the value drops under "Default currency" at the end rather than squeezing the title; nothing clipped.
- [x] TalkBack on the row → "Default currency, Australian dollar, button" (with XMR: "Monero (XMR)"). (content-desc reads the full name)
- [x] Open the list → title "Default currency"; the first group "MONERO" with "Monero (XMR)" and "Fob never looks up a price"; other rows keep their full names; search "monero", "xmr", "ɱ", "franc", "A$" each find what they should; "qqq" says no currency matches. ("ɱ" can't be typed with adb: covered by the Rust test (currency_list with "ɱ" gives MONERO))
- [x] Pick NZD while Price from is Kraken → the notice "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko".
- [x] XMR as default currency → Home's balance shows XMR only and tapping it does nothing (no ripple; TalkBack reads no button); Send, Receive's request amount and Pay's amount show no swap chip.
- [x] A currency with a price → tapping Home's balance swaps XMR and the currency; leave and reopen Fob → the order is kept; Send, Receive and Pay's chip swaps between XMR and that currency.
- [x] A currency but no price yet (airplane mode on a new wallet) → no swap on Home, no chip on the fields, the field line says there's no price. (checked with GBP in airplane mode: Receive in XMR with the price banner, then money after Retry)
- [x] Swap Home to the currency first, choose XMR, then choose the currency again → the currency leads again once its price is in.
- [ ] Install over round 7 with a wallet saved with Show first = the currency → it opens, Home leads with the currency; one saved with Show first = XMR leads with XMR. — needs a wallet file saved by a round 7 build

## Step 6 · Wallet code

- [x] Settings › Backup and keys → the phone's lock once, then the hub with its warning block. (with a PIN set on the emulator for the check, then cleared)
- [x] Hub › Wallet code → the QR shows at once with "Scan it in Fob, the Monero GUI or Cake Wallet." under it; no warning block, no Show the code or Hide button, no second lock prompt. Scan it from another Fob's Restore › Scan a wallet code → Wallet code found. (decoded with zbarimg (monero_wallet:…); scanning from a second Fob needs a second device)
- [ ] Same with a view-only wallet → the code at once, no "This code lets anyone see …". — needs a view-only wallet
- [x] Hub › Seed phrase → the words at once, no "These 16 words are your wallet …" block; Done goes back to the hub. With the backup still pending, the button reads "I've written them down" and Home's reminder goes after tapping it.
- [x] Hub › Keys → every key at once, as before.
- [x] Home's backup reminder (new wallet, words not written down) → the lock, then Seed phrase with the warning block above the words.
- [x] Create a wallet → "Back up your wallet" with the warning block, no lock.
- [x] On Wallet code, Seed phrase and Keys, try a screenshot → blocked; the recents thumbnail is blank. (adb screencap comes back black; recents thumbnail blank)
- [x] On Wallet code (and on Seed phrase from the hub, and from Home's reminder) press Home, reopen Fob → back on Settings (or Home), not on the secret; opening the hub asks for the lock again. Turning the phone keeps the code on screen. (from Keys: back on Settings, the hub asks for the lock again)
- [x] Light and dark, 200 % font → the code stays centred and fits; TalkBack reads "Wallet code" on the QR, then the scan line. — fixed in e530da7: at 200 % the scan line wrapped flush left under the centred code; it's centred now

## Step 7 · Layout shifts

- [x] Force-stop Fob, open it (a saved wallet) → the launch screen, then Home whole: balance, XMR line, photo, Receive/Send, Activity; nothing fills in after. As the chip turns Synced, "Balance so far" fades out and nothing moves. (screen record, frame by frame)
- [ ] Same with a funded wallet still catching up → "Still finding older ones" fades once synced; the payments don't move up. — needs a funded wallet (this one has no coins) still catching up
- [x] A new wallet with the backup pending, cold start → the reminder card is there on Home's first frame; open Settings and come back → it doesn't pop in again. "I've written them down" → it's gone on Home. (screen record of a cold start on a new mainnet wallet, removed afterwards)
- [x] Home › Settings (screen record, frame by frame) → the profile row, Backup and keys' line and the Node line are on the first frame; nothing moves. (screen record)
- [x] Node › change or check the node, Back → Settings' Node line shows the new verdict.
- [x] Home › Receive → the address, the field's sign, USD chip and "1 XMR = …" on the first frame; the code fills its square without moving anything. New address → the address and code change in place. (screen record)
- [x] A contact's card › Request → Receive keeps the chip's room; the chip fills it, the code doesn't move down.
- [x] Receive and Send with XMR as the default currency → no chip, an empty line's room under the field; type "x" → "That isn't an amount" appears without moving Note.
- [x] Settings › Default currency with no price yet (offline) → Receive opens in XMR saying why; once a price comes it switches to money in one step.
- [x] Home › Send → the viewfinder and Allow the camera appear once, in place; tap the handle → the sheet rises from its folded height, no flash of the form at full height.
- [x] Send with contacts or paid-before rows → the folded sheet is full height on the first frame; the viewfinder doesn't jump.
- [x] Home › Contacts and Home › Coins → the list (or empty card) is on the first frame; Coins' Split up greyed out with no coins. (screen records)
- [x] Light and dark, 200 % font → none of the above moves; TalkBack doesn't read the faded "Balance so far" once synced. (recorded at 100 % in light; 200 % and dark checked by screenshots; the faded lines are out of the accessibility tree once synced)
- [ ] A slow phone (or a large wallet) → the launch screen waits at most 2.5 s, then Home fills in. — needs a slow phone or a large wallet

## Step 8 · Split up Auto

- [ ] A funded wallet with 3 or more payments in the last 30 days › Coins › Split up → opens empty: every note "tap to add", no coins card, button greyed "Add a coin to split" with the fee line under it as before; the top bar has Auto (sparkle, outline) and no Clear. No "Picked from your last 30 days …" or "Not enough payments …" text; the intro "Each payment locks …" stays. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Tap Auto → the coins fill with the learned mix (at most 15, within the money), Auto lit (tinted, orange edge), Clear appears, the button reads "Split into N coins" with the fee under it. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Add or remove one coin → Auto goes plain; tap Auto → the mix comes back and it's lit again. Put the changed coin back by hand → lit again. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Clear → no coins, Clear gone, Auto plain. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Leave Split up and open it again (after Auto) → empty again. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] A wallet with 0–2 payments in the last 30 days (e.g. only received money) → no Auto button and no text about it; Clear appears once a coin is added. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Enough payments but only a tiny amount ready (less than the smallest note and the fee) → no Auto. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Light and dark → Auto plain is an outline on paper; lit is a warm tint with an orange edge, readable, no thick outline; the sparkle is orange in both. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] TalkBack → Auto reads "Auto, fill coins like your last 30 days, button"; once lit, "Selected" too; Clear reads "Clear". — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] 200 % font and a long title → Auto and Clear keep their 48 dp height and stay inside the screen; the title ellipsizes. — needs a funded wallet (this one has no coins) with payments in the last 30 days
- [ ] Prices in yen or dong (long notes) → Auto fills as before; the tiles' labels shrink to fit. — needs a funded wallet (this one has no coins) with payments in the last 30 days

## Emulator pass

Walked on emulator-5562 (Android 16, stagenet wallet with no coins) on
2026-11-01: 360 dp (1080×2400 at 480 dpi), light and dark, 100 % and
200 % font, landscape, the accessibility tree from uiautomator, screen
records read frame by frame, and every payment code decoded from a
screenshot with zbarimg. A mainnet wallet was made for the new-wallet
checks and removed after.

- Passed: 80
- Passed after a fix: 3
- Needs hardware or data this emulator hasn't: 27 (a funded wallet, a view-only wallet, a node that sends no blocks, a second screen or phone, NFC, a round 7 wallet file, a slow phone)

Fixes: 7929f98 (Home's top bar gives way chip words, then "Fob", then the
badge), afa340a (label and value rows wrap the label first), 2cbde51 (the
orange button's label padded inside its pill), e530da7 (Wallet code's scan
line centred).

## Review fixes

Six reviewers' findings and the emulator pass's (job folder `review8/`),
each fixed in:

| Id | What | Commit |
|---|---|---|
| H1 | Receive's address handed out only as Receive opens; a refresh reads it without handing out | 2607405 |
| H2 | Payment codes v11 at least, H to v20 then Q, the square holds at most one alignment pattern, dots placed by Rust | e01fdaf |
| M1 | One read of the screens at a time; New address and the hand-out under it; later asks share the waiting read | 2607405 |
| M2 | A read of a wallet closed meanwhile is dropped, not a crash | 2607405 |
| M3 | One `screens` call, the activity built once, the word count from the keys | 2607405 |
| M4 | The seed phrase warns until written down, from every way in; the words from Rust | 2607405 |
| M5 | Home's top bar: "Fob", then the badge, then the chip's words | e4ff4b9 |
| M6 | No bands on a synced Home: "Balance so far" gone, "Still finding older ones" last in Activity | e4ff4b9 |
| L1 | PhotoSheet's doc | 319752d |
| L2 | Unused imports and Kotlin warnings in round 8's files | 319752d |
| L3 | Scan a profile's title on one line | 319752d |
| L4 | Balance's worth and swap from one look | 2607405 |
| L5 | A price asked while the balance swaps is kept | 319752d |
| L6 | Split up's Auto payments read straight from the wallet | 319752d |
| L7 | A label takes the whole line once its value goes under | 319752d |
| L8 | Split up's footer empty with no coin chosen | 319752d |
| L9 | A dark starting window at night | 319752d |
| L10 | Receive shows Rust's words when it can't hand out an address | 2607405 |
| L11 | The code's dots from the address in the code itself; the encode remembered | e01fdaf |
| L12 | A new wallet's words close when Fob leaves | 2607405 |

Checked on emulator-5562 (Android 16, the stagenet wallet with no coins,
and a mainnet wallet made for the backup checks and removed after):

- [x] Receive, light: the code reads in zbarimg; New address 4 times, each new code reads (light) → done, 5 of 5
- [x] The same in dark → 4 of 4 read
- [x] A new wallet's Receive, recorded: the code's square and the address's lines are kept from the first frame, the address comes with nothing moving → done
- [x] Home synced at 360, 393 and 412 dp, 100 % and 130 % light, 200 % dark → no band under the top bar or over TODAY; "Synced" keeps its word everywhere, "Fob" shows at 412 dp/100 %, the badge goes at 360 dp/130 %
- [x] Cold start in dark, recorded frame by frame → the starting window is dark (the splash on the dark paper), Home's first frame whole, "Still finding older ones" at Activity's end while starting, nothing above it moves
- [x] Backup and keys › Seed phrase on a backed-up wallet → no warning
- [x] A new wallet's backup skipped: Backup and keys says "not written down yet" and Seed phrase has the warning with "I've written them down"; tapping it → the line says "backed up" and Seed phrase opens with no warning
- [x] Back up your wallet, Home key, Fob again → Home with its reminder, not the words
- [x] Switching network 5 times while mainnet catches up (each within 2 s) → no crash, same process
- [ ] Split up with no coin chosen → no fee line (needs a coin ready: this wallet has none; Rust tests cover it)
- [ ] Receive's code from a real phone camera with a ZBar-based app (Linux, F-Droid scanners) and with Google Lens → reads first time, several addresses
- [ ] Mainnet wallet catching up at 393 dp with "6 min left" → the time stays, "Fob" goes
- [ ] TalkBack on Receive while the address comes → the unseen stand-in isn't read
- [ ] Dark mode with Fob's theme set to Light, cold start → a dark window for a moment (the window follows the phone's mode), then light Home
