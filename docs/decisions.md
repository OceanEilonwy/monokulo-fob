# Decisions

Calls made while building the first version, with why. Newest last.

## Shape

1. **All wallet logic is in Rust (`crates/fob-core`); the Android app is a
   thin layer.** Keys, seeds, syncing, scanning, sending, fees, payment and
   wallet codes, ORF receipts, the wallet file format, amount and money
   formatting, and QR modules are all in fob-core. `crates/fob-ffi` only
   converts types for UniFFI. The app draws screens, runs the camera and the
   ZXing QR reader, keeps the wallet file's key in the Android Keystore, asks
   for a fingerprint before sending, and formats dates. iOS will reuse
   everything below the screens.
2. **monero-oxide (`monero-wallet` 0.2) for scanning and transactions**, not
   wallet2. View tags are used by its scanner: each output's view tag is
   checked before the full derivation.
3. **Seeds: Polyseed (16 words) for new wallets**, 25-word seeds accepted for
   restore. `third_party/` vendors `monero-seed` and `polyseed` from
   kayabaNerve/monero-wallet-util at `797fe70` (MIT): the crates.io
   `monero-seed` is an empty placeholder, and upstream pulls `std-shims`
   from serai's unpinned git branch. Only their `Cargo.toml` changed.
4. **ORF receipts through Monokular's `orf-codec`**, as a git dependency
   pinned to `e104a17` of OceanEilonwy/monokular. Licence: GPL-3.0-only to
   match it.
5. **Fob has no Monokulo features**: no Monokulo node, connect codes or
   dashboard links. Only the look comes from the Monokulo family.

## Syncing fast

6. **Fob reads `get_blocks.bin` itself (`fetch.rs`).** monero-oxide's daemon
   client gives up on a `get_blocks.bin` response whose transactions carry an
   all-zero `prunable_hash` (monero-project/monero#10120), which the public
   stagenet and mainnet nodes send, and falls back to two or three JSON calls
   per block: about one block a second, 90 minutes for a week. Fob doesn't
   need the prunable hash (the block lists its transactions' hashes, as
   wallet2 relies on), so it accepts it. It still checks every block follows
   the one before and that each block's transaction count matches.
7. **Its own HTTP transport (`transport.rs`)**: reqwest with rustls, pooled
   keep-alive connections, requests in parallel. monero-oxide's
   `simple-request` transport opens a connection per request and holds a lock
   so only one runs at a time.
8. **100 blocks a request, 3 requests in flight, 600 blocks a sync step.**
   Measured on public nodes (UkoeHB/monero#23 used 20-block chunks, 10 in
   flight, against a local daemon): monerod limits RPC connections per IP and
   resets the fourth or fifth parallel one; bigger chunks are fewer round
   trips. A week of stagenet (5,040 blocks) syncs in 8–35 s depending on the
   node; scanning it takes under a second. Mainnet is limited by the node's
   bandwidth: a day (720 blocks, 27k transactions) took 58 s on
   node.monerodevs.org.
9. **Reorgs are found for free**: a fetched batch whose first block doesn't
   follow the last scanned one rewinds the last 30 blocks. The tip height is
   asked for only near the end of what's known.
10. **A new wallet starts at the chain's tip** (less 10 blocks); a Polyseed
    restore starts at its birthday (rounded down to the month by Polyseed); a
    typed date starts a week earlier; a 25-word seed with no date starts at 0.
11. **Default nodes**: stagenet `stagenet.xmr-tw.org:38081` (fastest in
    tests), then `node.monerodevs.org:38089`, `node2.monerodevs.org:38089`;
    mainnet `node.monerodevs.org:18089` (https), sethforprivacy, Cake. All
    editable in Settings › Node.

## Sending

12. Inputs are chosen largest first, with decoys from the node (ring 16,
    CLSAG + Bulletproofs+), change to the primary address, and the node's
    Normal fee, capped at 2,000,000 piconero per weight unit so a hostile
    node can't take a large fee.
13. **Swipe to send, then fingerprint (or screen lock).** On a phone with
    neither set up the swipe alone sends.
14. A sent payment's inputs are marked spent at once and its change counted
    in the balance until the transaction is mined.
27. **The output distribution is cached (`decoys.rs`).** Choosing decoys needs
    the cumulative RingCT output count at every block: about 18 MB on
    stagenet, 30 MB on mainnet, uncompressed. monero-oxide fetched it on each
    payment, which took almost 3 minutes from a public node. Fob keeps it in
    `<wallet>.decoys` beside the wallet (public data), tops it up while idle
    (re-fetching the last 30 blocks in case of a reorg), so preparing a
    payment only fetches its ring members.
28. **No overall request timeout, a 30 s idle one instead**: a large response
    from a slow node is fine as long as it keeps arriving.

## Safety

23. **Burning bug**: an output whose one-time key was already received is
    ignored, as monero-oxide's scanner asks; only the first counts.
24. **Secrets stay off screenshots**: the seed and wallet-code screens set
    `FLAG_SECURE`.
25. **The wallet file** is ChaCha20-Poly1305 under a random 32-byte key, which
    is itself wrapped by an AES key kept in the Android Keystore.

## Left for later

15. **Mempool, done differently**: once caught up, Fob asks the node for its
    pool and scans new pool transactions inside a stand-in block (the last
    one scanned, with the pool's transactions as its own), because
    monero-oxide only scans blocks. Their amounts show as on their way, in
    memory only; the real block scans them again with true output indexes.
16. **Background sync**: see item 35.
17. **Google Wallet**: not built. Each pass is a JWT signed with a Google
    Wallet issuer's key, which can't ship in the app, so it needs an issuer
    account and a small signing service. Settings says so.
18. **Fiat amounts**: none yet. Fetching rates means asking a price service,
    which tells it when you use your wallet.
19. **Subaddresses**: account 0 only, 50 watched past the last one handed out.
20. **ABIs**: arm64-v8a (phones) and x86_64 (emulator) only.
21. **Pipelining across sync steps**: done, item 30.
22. **Tested on stagenet, 2026-10-31**: a faucet payment received by
    fob-core; 0.03 XMR sent from fob-core to the app's wallet (tx `41e7a708…`)
    and seen by the app in the pool before it was mined; 0.01 XMR with an ORF
    receipt sent from the app with swipe to send (tx `425cec27…`, prepared in
    4 s, sent in 4 s) and seen by the other wallet in the pool. Not tried yet:
    restoring in the app from a wallet code another wallet made, the camera
    on a real phone, mainnet.

## Dependencies

26. **Two weeks old, except security fixes.** Every crate in `Cargo.lock` was
    released by 2026-10-17, except `rustls-webpki` 0.103.16 (fixes
    RUSTSEC-2026-0382) and `webpki-roots` 1.0.10 (the oldest without
    RUSTSEC-2026-0381). Android libraries are Monokular's vetted versions plus
    `androidx.biometric` 1.1.0 (2021).
29. **Balances and lists show four decimals, cut not rounded**; what's about
    to be paid (confirm screen, payment codes) shows every digit.

## Faster still (branch `speed`)

30. **A pipeline across sync steps (`fetch::Pipeline`).** A task keeps
    `get_blocks.bin` requests in flight in order and hands finished chunks
    through a short queue, so fetching goes on while the wallet scans and
    saves. A step scans whatever has arrived, up to 1,000 blocks. Replaces
    item 8's fixed rounds.
31. **Adaptive chunks and parallelism (`fetch::Adaptive`, `fetch::Tuning`).**
    Chunks aim at 2 MB of response from the bytes per block seen (20 blocks
    first, so a slow node with full mainnet blocks answers quickly; 10-500
    after). Requests in flight start at 3, add one after twice that many good
    answers, up to 8, and drop for good below a level the node refused. A
    node of your own climbs to 8; public ones settle where they stop
    resetting. Results: a week of stagenet 10-22 s (was 36 s on the same
    node); a day of mainnet 21 s on sethforprivacy, 47 s on monerodevs.
32. **Stale keep-alive connections are retried in the transport**, for every
    request, and idle pooled connections are dropped after 8 s: monerod
    closes them, and reusing one failed the request.
33. **The output distribution comes compressed** (`compress`, per-block
    counts as varints, summed on top of `base`): about one byte a block
    instead of eight. Checked equal to monero-oxide's uncompressed answer.
34. **Node speed test**: fob-core checks a node (reachable, its network,
    synced, latency, blocks a second through the real fetcher); the Node
    screen tests the current and public nodes and marks the fastest.
    Setting a node on the other network is refused.
35. **Background sync (`SyncService`)**: a `dataSync` foreground service
    started while Fob is on screen and more than about a day behind (Android
    doesn't allow starting one from the background). It shows progress and
    stops itself once caught up; off screen, syncing then stops. The wallet
    now lives in one app-wide `WalletModel` that the service shares, so the
    file is never open twice. Tested: a month of stagenet restored, Home
    pressed at 10 s, caught up in the background by 20 s, service gone.

## Look

36. **The mark is the Fob seal**: the Monokulo iris pressed into a disc of
    orange wax with a calm, five-lobed edge, after the fob seals that hung on
    watch chains and signed letters. It replaces the lens with a watch crown
    and bow, which shrank to a blob at launcher size. One formula draws it:
    `docs/brand/seal.py` writes the launcher, themed and status-bar icons and
    the SVGs in `docs/brand/`, and `FobMark` uses the same edge. The themed
    icon has its own one-colour drawable, with the ring and blades cut
    through the wax; before, it reused the colour foreground, which Android
    flattens to a plain disc.
37. **In the wordmark the seal is the o**, placed from measured edges of the
    f and b in Manrope ExtraBold: 0.125 em from the f, 0.1 em to the b
    (the f's crossbar points straight at the seal, so equal gaps looked
    tight on that side), and 1.24 x-heights tall, centred on the x-height.

## Second round of designs

36. **Amounts drop trailing zeros everywhere**: "0.5", "0.653", "1", never
    "0.5000". Balances and lists are still cut (not rounded) to four
    decimals, as item 29; `amount::format` no longer pads.
37. **Activity is grouped by day in Rust (`activity::by_day`).** The app
    passes the phone's time, its UTC offset and whether it uses a 24-hour
    clock; Rust returns the day labels ("TODAY", "YESTERDAY", "27 OCT", the
    year when it isn't this one) and each row's title, one line of status,
    receipt and time, and signed amount. So item 1's "the app formats dates"
    no longer covers Activity. One offset serves every row: a payment from
    the other side of a daylight-saving change can land an hour off near
    midnight. A row still unconfirmed from yesterday goes under yesterday.
    Incoming money under 10 confirmations reads "Not ready · 3 of 10";
    outgoing "Confirming · 3 of 10"; a settled payment with a receipt says
    "Receipt, 3 items" rather than "Sent".
38. **Pull to refresh calls `Wallet::sync_now`**: it wakes the sync loop
    from its 20 s wait (or its backoff after an error) and returns when a
    round ends caught up or failed, at most 20 s later. With no loop running
    it does one step itself.
39. **Dark mode.** Colour roles are a `FobColors` per theme behind a
    CompositionLocal; `Fob.ink` and the rest read the current one, so screens
    keep naming roles. The orange, the text on it and what's drawn over the
    camera are the same in both. QR codes are always dark on a white card.
    The choice ("Same as the phone", Light, Dark) is an Android preference
    beside the network, not in the wallet file: it is only how Fob looks,
    the same for every wallet, and is needed before any wallet opens.
    Dialogs use the card colour instead of Material's tinted default.
40. **Seed words never wrap**: two columns numbered down (1–8, 9–16; 13 and
    12 for 25 words), one column from a font scale of 1.6.
41. **"0.5 XMR ready to spend"** shows under the balance only while some of
    it can't be spent yet (`Balance::ready_text`).
