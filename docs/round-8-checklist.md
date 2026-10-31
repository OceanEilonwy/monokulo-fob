# Round 8 device checklist

Added to by each step of `round-8-plan.md`, walked through on an emulator once
every step is in. Each item: what to do, what should happen.

## Step 1 · Texts

- [ ] Contacts with a few contacts and a Paid, not saved row → nothing under the last card; light and dark.
- [ ] Contacts with none → "No contacts yet. Tap + to add someone you pay…" still shows.
- [ ] Receive with a profile and "Name in payment requests" on → no "The code names you as …" row under the note; Share still puts the name in the code.
- [ ] Receive → the address pattern shows alone (no "The payer's Fob shows this pattern too"), no "Address N · …" line; New address sits at the right end and still makes a new address.
- [ ] Receive with a contact picked, then Share → no "Address N · this request's own".
- [ ] Receive › Share → Who is this for? sheet: title, contacts, New contact, Share without a name; no explanation line.
- [ ] Receive › tap the code → full screen code and pattern, no brightness line; screen still goes bright.
- [ ] Split up with a fresh price → footer "Network fee about $0.03" only.
- [ ] Split up with a price over an hour old (airplane mode, clock forward) → "Network fee about $0.03 · 2 h old price"; nothing about XMR or today's price.
- [ ] Split up with XMR only → "Network fee about 0.000… XMR", no age.
- [ ] Split up's "Not enough payments yet…" / "Picked from…" tip still shows (step 8 replaces it).
- [ ] Settings › Node › check a node → "Sends blocks to Fob" shows "31 blocks/s" with the link speed on its own line under it, right-aligned, on a 360 dp phone and at 200 % font; never side by side.
- [ ] Same with TalkBack → "Sends blocks to Fob: 31 blocks/s, 1.8 MB/s".
- [ ] A node that sends no blocks → one red line "no blocks in 10 s", no empty second line.
- [ ] Home › sync chip while catching up → Time left, Payments up to, Blocks, Link speed rows; no "You can still receive …" note. With the node off → "Fob keeps trying on its own." still shows.
- [ ] Settings → Contacts row says "Who you pay"; Use mainnet on says "Real money"; Receipts row has no "on this phone only".
- [ ] Settings › Instant updates → Status, ZMQ address field and hint; no paragraph under it.
- [ ] Send → Paste an address row is one line, still centred in its 60 dp row; note placeholder "What it's for".
- [ ] Contact edit › photo, and Edit your profile › photo → the sheet has Take, Choose, Remove and no footnote.
- [ ] Edit your profile (and onboarding step 3) → intro is one sentence; no help line under Your name.
- [ ] Your profile → Name in payment requests switch has no line under it (title stays on one line, switch reachable); Your shares header has nothing at its right.
- [ ] Share my profile → no "A new address for this share…" under the code.
- [ ] Create a wallet → Back up your wallet shows the words and warning, no Polyseed line.
- [ ] Restore › View-only wallet → fields start right under the top bar.
- [ ] Restore › Scan a wallet code › found → no Keystore sentence under the card.
- [ ] Backup and keys › Keys → Main address has no line under it; the others keep theirs.

## Step 2 · Edge buttons

- [ ] Settings › Node → the refresh sits clearly in from the right edge (centre about a thumb's width in), bigger than before; tap it near its edge → checks again and turns; TalkBack reads "Check again", "Checking" while it runs.
- [ ] Any screen with Back (Settings, Coins, Contacts) → Back sits in from the left edge as far as Node's refresh from the right; TalkBack reads "Back".
- [ ] Home → the photo (40 dp) and the cog sit in from the right edge, both easy to hit; TalkBack reads "Your profile" (or "Add a shareable Fob contact") and "Settings". No profile yet → the grey person circle at the same size.
- [ ] Home on a 360 dp phone, stagenet, while catching up ("6 min left"), and at 200 % font → photo, cog and the chip's ring stay whole and on screen; "Fob" and the chip's words drop first.
- [ ] Send (camera) → Close top left and the torch top right at the same inset and size as top bar buttons; torch toggles, TalkBack reads "Torch, Off/On".
- [ ] Restore › Scan a wallet code, Contacts › Scan a profile, Contact edit › scan an address → Close in from the left, white on the camera, TalkBack "Close".
- [ ] Receive › tap the code, Share my profile, and Touch phones → Close in from the left at the same size.
- [ ] Contacts → Scan a profile and Add a contact side by side at the right, each 56 dp; TalkBack reads both labels.
- [ ] A contact → Edit; Your profile → Edit; Split up → Clear; Contact edit → Save: each in from the right edge, easy to tap, at least 48 dp tall.
- [ ] Onboarding Edit your profile (step 3 of 3) → Skip in from the right edge.
- [ ] 200 % font on a 360 dp phone → titles (Backup and keys, Type your seed phrase, Contact edit's title beside Save) stay on one line and end in "…"; Back and the actions keep their size; nothing overlaps.
- [ ] Landscape with a cutout or the gesture bar at the side (Home, Node, Send's camera, Receive's full screen code) → every edge button stays inside the safe area, none under the cutout.
- [ ] Light and dark → buttons keep their usual ink (white on the camera, dark on the white code screens); no new outlines or fills.
- [ ] Dialog buttons (Switch network, Remove wallet, Theme) → still sit right, now at least 48 dp tall; dialogs look unchanged otherwise.

## Step 3 · Receive's code

- [ ] Receive, no amount, light mode → the address's dots sit in a white square in the code's middle; no pattern row under the card; another phone or app (Google Lens, a second wallet) scans it to the right address.
- [ ] Same in dark mode → the card and the square stay white, the dots in their colour; it scans.
- [ ] Type an amount → the code redraws, dots still centred; another app scans it with the amount.
- [ ] Amount and a short note, then a 120-character note (and one in emoji) → the code gets denser but the dots stay centred and about the same size; each scans with the note.
- [ ] Tap the code (or its corner button) → full screen code with the dots in its middle and no pattern under it; scans at arm's length in light and dark; Close returns.
- [ ] Request › Show the code again (with a name and a note) → the full screen code has the dots in its middle; it scans.
- [ ] New address → the dots change with the address.
- [ ] TalkBack on Receive's card → reads "Payment code" and offers "Show larger"; the dots aren't a separate stop.
- [ ] Share my profile → its code is unchanged (no square in the middle), still scans with the photo.
- [ ] Backup and keys › Wallet code → unchanged, no square; still scans.

## Step 4 · Coins and warnings

- [ ] New wallet, no coins → Coins shows Split up at the bottom, greyed out (sunken fill, muted words and icon); tapping does nothing; TalkBack reads "Split up, button, disabled".
- [ ] Only a payment not ready yet (0–9 confirmations) → Split up still greyed out; once it's ready (or within 30 s of the coin turning ready) it turns orange and opens Split up.
- [ ] Open Coins with ready coins, watching closely (or a screen record) → Split up goes from greyed to orange; never orange first then grey.
- [ ] View-only wallet → Coins shows no Split up at all, as before.
- [ ] Light and dark → the greyed Split up reads clearly as off, its words legible.
- [ ] Backup and keys, light then dark → the warning sits on a clear peach (light) or deep brown (dark) block, words in full ink at body size, orange triangle; no edge or outline.
- [ ] Seed phrase (from the hub and from Home's reminder) → "These 16 words are your wallet …" in the same block above the words; a view-only wallet's "This wallet only watches …" stays plain.
- [ ] Wallet code, full and view-only wallet → each warning in the same block.
- [ ] Home's backup reminder (new wallet) → same block with the lines icon, title, line in ink and a chevron; tapping it opens the seed after the lock; TalkBack reads it as one button.
- [ ] Settings › Network › Use mainnet → the dialog's text in the warning block; Back to stagenet stays plain text. Settings › Remove this wallet → the warning block.
- [ ] Contacts › add a contact, paste an address that replaces one → the "replaces" warning in the same block, Use it and the other button below.
- [ ] Restore › Scan a wallet code → the warning card on the camera keeps its dark card and light words, same layout as the others.
- [ ] 200 % font → every warning wraps inside its block, the triangle stays at the top left, nothing clipped.

## Step 5 · Default currency

- [ ] 360 dp phone (or emulator at 360 dp), default font, light then dark → Settings › Money shows "Default currency" with "USD · $" (or the chosen one) and a chevron on one line; no line under the title; no "Show first" row; Price from under it.
- [ ] Choose AUD, EUR, JPY, CHF, NZD in turn → the value reads "AUD · A$", "EUR · €", "JPY · ¥", "CHF", "NZD · NZ$"; never a full name, never wrapped.
- [ ] Choose Monero (XMR) → the value reads "XMR"; Price from disappears.
- [ ] 200 % font → the value drops under "Default currency" at the end rather than squeezing the title; nothing clipped.
- [ ] TalkBack on the row → "Default currency, Australian dollar, button" (with XMR: "Monero (XMR)").
- [ ] Open the list → title "Default currency"; the first group "MONERO" with "Monero (XMR)" and "Fob never looks up a price"; other rows keep their full names; search "monero", "xmr", "ɱ", "franc", "A$" each find what they should; "qqq" says no currency matches.
- [ ] Pick NZD while Price from is Kraken → the notice "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko".
- [ ] XMR as default currency → Home's balance shows XMR only and tapping it does nothing (no ripple; TalkBack reads no button); Send, Receive's request amount and Pay's amount show no swap chip.
- [ ] A currency with a price → tapping Home's balance swaps XMR and the currency; leave and reopen Fob → the order is kept; Send, Receive and Pay's chip swaps between XMR and that currency.
- [ ] A currency but no price yet (airplane mode on a new wallet) → no swap on Home, no chip on the fields, the field line says there's no price.
- [ ] Swap Home to the currency first, choose XMR, then choose the currency again → the currency leads again once its price is in.
- [ ] Install over round 7 with a wallet saved with Show first = the currency → it opens, Home leads with the currency; one saved with Show first = XMR leads with XMR.
