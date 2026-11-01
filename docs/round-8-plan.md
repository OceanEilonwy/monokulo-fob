# Round 8: polish after the round 7 phone test

The owner tried round 7 on a Pixel 9 Pro (2026-11-01) and asked for twelve
fixes. Their numbering is kept below as "item N"; the steps group them.

## Decisions (the owner's)

1. Node: the link speed goes on its own line under the blocks per second,
   never wrapping on the same line.
2. Action buttons at the screen's edges (Node's refresh, the scanner's X,
   Home's profile photo and Settings cog, and every other top bar action and
   Back) sit further in from the edge and are bigger: easier to hit.
3. Home has a momentary layout shift as it starts up: find it on the
   emulator and fix it.
4. Remove noise text. Named by the owner: Contacts' "Contacts live in this
   wallet's file, …", Receive's "The code names you as “Henry”, …", Split
   up's "coins are set in XMR at today's price". Sweep the rest of the app
   the same way (see step 1).
5. Receive: the identicon (the address's pattern) goes in the middle of the
   QR code. Remove "The payer's Fob shows this pattern too" and "Address N, a
   new one each time".
6. Coins: with no coins ready, Split up is shown greyed out and can't be
   pressed (not hidden).
7. Split up: remove "Not enough payments yet …". Only when there's enough
   to learn from, an "Auto" button fills the learned mix. Designs on the
   canvas, page Round 8; built once the owner picks.
   Picked A: Auto in the top bar beside Clear; the page as drawn.
8. Opening Settings or Receive shifts the layout: fix it.
9. "Also show amounts in" becomes "Default currency"; "Show first" goes. With
   XMR selected, no swap button anywhere. With a currency selected, the swap
   button shows and swaps between XMR and the default currency.
10. The Default currency row must not wrap: a short value (code and symbol,
    not the full name) and a shorter description.
11. Warnings like "Anyone who sees these …" are too faint: stronger styling.
12. Wallet code shows no warning screen of its own when opened from Backup
    and keys, which already warned.

## Steps

1. **Texts** (items 1, 4, 5's texts): remove the named texts; sweep every
   screen for sentences that explain how Fob works, reassure, or describe
   what the other side sees, where the screen's labels already say enough,
   and remove them. Keep warnings that stop someone losing money or keys,
   errors, and empty states that say what to do. Log each removal in
   decisions.md with where it was. Node's "Sends blocks to Fob" shows
   blocks/s and the link speed on separate lines.
2. **Edge buttons** (item 2): one shared size and inset for top bar
   actions, Back, Home's photo and cog, and the scanner's close button.
3. **Receive's code** (item 5): the identicon inside the QR's centre,
   with error correction that keeps it scannable (Rust test decodes the code
   with the centre covered).
4. **Coins and warnings** (items 6, 11): Split up disabled with no coins
   ready; stronger warning style through theme roles in light and dark.
5. **Default currency** (items 9, 10): rename, drop Show first, swap only
   with a currency, short value; old wallet files open.
6. **Wallet code** (item 12).
7. **Layout shifts** (items 3, 8): find each on the emulator (frame-by-frame
   screenshots or screen record) and fix the cause.
8. **Split up Auto** (item 7): the owner's pick from the Round 8 page.

Then, as in rounds 3 and 7: emulator walk-through of
`docs/round-8-checklist.md` with a fix pass, an independent review of each
commit, one assembled fix pass, PR, green CI, merge.
