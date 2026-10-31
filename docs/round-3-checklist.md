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
- [ ] Send → still the XMR-only amount field (moves to toggle B in step 4).
