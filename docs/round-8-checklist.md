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
