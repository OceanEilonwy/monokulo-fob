# Round 3 plan

The work agreed after the second device test, in build order. Designs are on
the Fob canvas (https://claude.ai/artifact/MK3rLvr8rcGa6WA5CZrXat), named here
as **page › board**. Every step keeps wallet logic in fob-core; the Android app
only draws, runs the camera, NFC and OS services, and calls Rust.

## Decisions taken before building

1. The profile photo is encoded by Android's own WebP encoder; Rust only checks its size.
2. A payment request is made when Share or Copy is pressed on Receive, never by just opening Receive.
3. Settings › "Show amounts in" stays a segmented control; amount fields use toggle B.
4. The profile photo budget is 1.5 KB (96 × 96 WebP, quality stepped down to fit, then smaller).
5. A rescanned profile that would change an existing contact's name or photo always asks first.
6. Pressing Share my profile always records a share, however briefly the code was shown.

## Steps

1. **Home rework**: Round 4 › "Home · balance C2, Coins chip on Activity", "Home · balance C2 opened"; Shared pieces › "Home buttons (Receive, Send)".
   One-line balance, the folded ready / not-yet card opening to sums, Receive (outline) and Send (orange); Scan to pay and the Split up links go.
2. **Coins screen**: Round 4 › "Coins A (picked) · thin bar, Split up button", "Split up: where it lives (D picked, now a Coins chip)".
3. **Currency toggle B with ɱ**: Round 3 › "Toggle B (picked, approved) · $ becomes ɱ"; Shared pieces › "Amount field with toggle B".
4. **Send B**: Round 3 › "Send B (picked) · sheet with contacts", "Send B · sheet pulled up (A's form + Note)", and the safe-area note.
5. **Contacts, data**: Round 5 › "Sam: send address, requests, also paid at"; Round 4 › "Contacts".
6. **Contacts, screens**: Round 4 › "Contacts", "Delete, asked first"; Round 5 › "Sam: send address, requests, also paid at", "Edit: photo, name, note, send address", "Scan their code for the send address", "Contact photo: take, choose, remove"; Shared pieces › "Contact avatar (photo or initial)".
7. **Saving a contact after paying**: Round 4 › "Sent: a small Save contact button", "New contact, from Save contact"; Round 5 › "Paid a code named Sam: add to Sam", "Paid a code named Henry: save as new".
8. **Requests**: Round 5 › "Share asks who it's for", "Request for Sam: a new address each time", "Activity: requests and payments, separate lines", "A request: no edits, can be deleted".
9. **Your profile**: Round 6 › "New wallet, last step: make it yours (optional)", "Your profile: every share listed", "Share my profile: code, or touch phones", "Activity: each share is a line", "Another Fob scans the code".
10. **Touch-phones swap (NFC)**: Round 6 › "Touch phones", "Swapped: both got each other", and its note.
11. **Settings**: Round 2 · Money › "Settings: display, prices, node" (Your profile row, Contacts entry).

Each step is committed on its own with unit tests where they apply, and adds
what to check on the device to `docs/round-3-checklist.md`, which is walked
through on an emulator once every step is in.
