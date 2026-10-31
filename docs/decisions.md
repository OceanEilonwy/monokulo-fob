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
42. **Instant updates over ZMQ (`zmq`, `zmtp`).** Monokulo's ZMTP 3.0
    subscriber is copied into fob-core, not shared: Fob has no code ties to
    Monokulo, and the files say where they came from. Kept: bounded frames
    (64 MiB, read as they arrive, never allocated from a declared size), a
    10 s handshake limit, a 10-minute idle limit with TCP keep-alive, a
    broken protocol just ending the connection, and reconnecting after 1 s
    doubling to a minute. Dropped: `ipc://` (a phone has no node on a Unix
    socket) and the engine's status figures. The test publisher came too,
    so tests run against a fake monerod in-process; the two proptest
    properties run as loops over a seeded generator rather than adding
    proptest as a dependency.
43. **Off by default; the node's own host only.** The setting (on or off,
    and the address, kept while off) lives with the node in the wallet
    file. Rust checks the address: `tcp://host:port` (or `host:port`), no
    user, path or query, and the host must be the node's. Fob offers
    `tcp://<node host>:18083` (mainnet) or `:38083` (stagenet) and connects
    nowhere until it's turned on. Changing to a node on another host turns
    instant updates off rather than following somewhere not chosen.
44. **What an announcement does.** `chain_main` wakes the sync loop at once
    through the same wake `sync_now` uses; connecting does too, for blocks
    missed meanwhile. `txpool_add` has the waiting loop look at the pool
    (once caught up), 2 s after the announcement so a burst is one look,
    and the wait carries on. Polling stays as the fallback: every 20 s as
    before, every 2 minutes while the publisher is heard.
45. **ZMQ status comes with the sync status**: Off, Connecting, Listening
    or Can't connect with a few words of why, and Settings' subtitle as
    Rust writes it ("host:18083 · listening"). A change is reported to the
    app like any other sync step. On but not syncing reads "connecting":
    it connects again when syncing starts. Turning the switch on with no
    address Rust accepts opens the address screen instead.

## Third round: money not ready, the address pattern, Receive

46. **Money not ready yet is shown per output, not per payment
    (`not_ready`).** A piece is an unspent output short of its
    confirmations (10, or 60 for a mined reward), the change of a payment
    not yet mined (0 confirmations), or a payment to this wallet still in
    the pool (0 confirmations; the pool is kept per transaction, so one
    piece per transaction there). These are exactly the money `Balance`
    counts as not unlocked, and a test holds them equal.
    **Unlock times are honoured now.** The scanner used to drop every
    output whose transaction named an unlock time, which (unnoticed) meant
    mined rewards too, whose unlock time is 60 blocks on. Outputs locked to
    a block are now kept with that block (`unlock_block`), and need
    max(10, or 60 for a reward; unlock block − their height) confirmations,
    for the balance, for choosing what to spend and for this list alike.
    Outputs locked to a time are still not kept: the phone's clock can't
    say when they unlock. Rewards already passed by an existing wallet only
    appear after a restore from an earlier height.
47. **Where a piece came from**: "Received · 08:40" (when it arrived), or
    "Change · Corner Café" / "Change · 86Ux … 7kQa" when it is change from
    one of this wallet's own payments, named as Activity titles it; change
    from a spend found on the chain that this phone didn't send (a restored
    wallet) is just "Change".
48. **When it will be ready**: about 2 minutes a remaining block, counted
    from the last block scanned if it came within the last 2 minutes, else
    from now (the wallet is catching up, a block is late, or the phone's
    clock is behind the block's). "09:02" is the phone's time of day only,
    with no date (a 60-block reward can cross midnight; rare enough).
    "in 14 min", rounded up, at least "in 1 min", then "in 1 h 20 min".
    Pieces are listed soonest ready first. Home asks Rust again every 30 s
    while the card shows, so the times count down between blocks.
49. **The summary**: "1.253 XMR not ready yet", "3 pieces · all ready about
    09:07" ("1 piece · ready about …"), and progress weighted by amount,
    Σ(amount × confirmations ÷ needed) ÷ total, shown as a whole percent
    rounded down. The old "confirming" pill and `Balance::confirming` are
    gone.
50. **The card**: folded by default under the balance; a button with
    expanded/collapsed state for TalkBack; the bar and each ring report
    themselves as progress. Green is `Fob.green` (not the brighter
    `greenFill`), which keeps 3:1 against the line colour in light mode.
51. **A payment's own screen takes its status from Rust**
    (`activity::status`): Activity's words while it waits ("Sending",
    "On the way", "Confirming · 3 of 10", "Not ready · 3 of 10"), then
    "Confirmed · 24". The app's `settled` flag went with it.
52. **The address pattern (`pattern`)**: dots on a 5 × 5 grid, mirrored
    left to right, in one of eight colours, on a white badge; no words, and
    only Fob draws it. Shown beside the address on Receive, under the
    expanded QR, and on the confirm screen before sending, so payer and
    payee can compare at a glance. It is a quick check, not a proof: with
    8 × ~32,000 patterns a deliberately made look-alike address is cheap to
    find, so it catches mistakes and lazy swaps, and the confirm screen
    still shows the address.
53. **Its derivation**, exactly, so another app could draw the same:
    `h = Keccak-256(address as UTF-8 bytes)` (Monero's Keccak, from the
    `sha3` crate already in the tree through monero-oxide, 0.10.9; not
    SHA3-256); `cells = (h[0] | h[1] << 8) & 0x7FFF`, bit `3 × row +
    column` for rows 0–4 top down and columns 0–2 from the outer left;
    `colour = h[2] & 7`. If fewer than 3 or more than 12 cells are on,
    `h = Keccak-256(h)` (the 32 bytes) and again from `cells`; about 0.7%
    of addresses take a second round. Drawn column `c` (0–4) shows source
    column `min(c, 4 − c)`. Rust returns the 15 bits, the colour index and
    the 25 dots as drawn. Vectors: the address
    `5AQmdbMiSRLJQqGGeVA2aMPoyRqR64sTb9BHKWdi8nBpTJTrRBsgffvPBsdm5JeSG6MRvG5SBPuNZtJjqDkJXapj5S6QWVc`
    gives cells 27718, colour 2; the empty string 0x52c5, colour 6;
    `fob-17` (2 cells at first) 28262, colour 2 after hashing again.
54. **Drawn**: grid side 66% of the badge, centred, each dot 84% of its
    cell, so the corner dots sit inside the circle; palette by index
    #B34700, #1F7A45, #2F5AA8, #7A3FA0, #A3324A, #1D6F7A, #5B5F17,
    #3D3A8C. The badge stays white in dark mode, with a 1 dp ring in the
    line colour so it doesn't vanish on a white card.
55. **Receive, second version.** The code sits on a white card (20 dp
    padding, 14 dp corners, no icon in the middle, about 220 dp) with a
    round 32 dp button on its corner; it or the code opens the code full
    screen: white in both themes, the amount large with the note under it,
    the code at 320 dp with a 4-module quiet zone, the address pattern,
    and the window's brightness at full until it closes (the window's own
    setting, put back after; no permission needed). The 32 dp button is
    below the 48 dp touch size, as drawn, since the whole code is a target
    too.
56. **The address in full, in groups of five (`uri::address_groups`)**:
    19 groups for a 95-character address, an integrated one split the
    same way with its last group shorter; the first, middle (index
    count ÷ 2) and last are highlighted, in two new theme roles
    (`addressMark` #FBE3CC / #4A2A12, `onAddressMark` #7A2F00 / #FFC9A0),
    bold. The four-character grouping it replaces (`full`, `Mono`) is gone.
57. **A note for the payer** goes into the code as `tx_description`, the
    name the Monero URI scheme and other wallets use. Rust builds the code
    (`uri::receive`, replacing `payment_code`): values percent-encoded
    except RFC 3986's unreserved characters, so a space is `%20` and a `+`
    `%2B`, which round-trips through Fob's own reader (which takes `+` as a
    space, as forms do). A blank note or amount is left out. On paying,
    the description shows on the Send form as before and now again under
    the amount on the confirm screen. The expanded code shows the amount
    in XMR large and the note under it; step 4 moves the fiat amount into
    the large line and the XMR beside the note.

## Fourth round: prices

58. **Provider and transport are chosen apart (`price`).** A provider is a
    trait, `price::Source` (its name, the currencies it lists, the price
    now, daily prices over a range, how far back it reaches); the setting
    names one through `price::Provider`, an enum dispatching to it, so the
    wallet file stores a word ("kraken") and adding a provider is a
    variant and a module. The trait isn't object-safe (its calls are
    `async`), which the enum makes moot; tests pass a fake `Source`. The
    transport is `price::Transport`: `Web` (HTTPS through reqwest and
    rustls, `https_only`, no redirects, 15 s to connect, 30 s in all, 1 MiB
    at most) or `Tor`, which fails every request with "prices over Tor
    aren't available yet" until an arti-based one is written. A Monokulo
    provider (asking the Monokulo app on the phone, through its app
    integration, for the price its till uses) has its marked place on
    `Provider`; it isn't built.
59. **Kraken (`price::kraken`).** Public REST, no key: `Ticker` for now (the
    last trade, `c[0]`), `OHLC` at 1440 minutes for history. Kraken lists
    XMR in two currencies of money, USD (`XMRUSD`, answered as `XXMRZUSD`)
    and EUR (`XMREUR`, `XXMRZEUR`), checked against `AssetPairs` on
    2026-10-31 (its other XMR pairs are USDC, USDT and XBT); the list is
    hard-coded and tested. A day's price is its volume-weighted average
    (nearer any moment of the day than the close), or the close for a day
    without trades. OHLC returns at most 720 days; older is "not
    available". One live test, ignored like the node tests, asks Kraken
    once each way; it passed on 2026-10-31.
60. **What a price request may say.** Nothing at all while "Show prices" is
    off. Only the pair, never anything of the wallet, with
    `User-Agent: Fob` and reqwest's bare defaults (no cookies). Never a
    price for a payment's moment: history is always the provider's whole
    window in one request (no `since`, which would give away the oldest day
    wanted), at most once a day, and only while some payment within the
    window lacks a price; days are looked up on the phone. The current
    price is asked at most every 5 minutes while Fob is on screen (a task
    started and stopped with the screen, woken at once by a price setting
    changing) and on pull to refresh, at most once a minute.
61. **Prices are whole numbers**: millionths of the currency per XMR, from
    the decimal text without going through floats. Values round half up to
    the cent. A current price older than an hour isn't shown or recorded.
62. **A payment's price is recorded with it in the wallet file**
    (`price_records`, by transaction, one record per currency: currency,
    price, provider, when the price is from, and whether it is a day's).
    When a payment is first seen (a sync step, the pool, sending) it takes
    the current price if that was asked within an hour of the payment's
    time; otherwise, or for one seen while prices were off or before
    prices, the day's price from the history (UTC days, as Kraken's
    candles), recorded once the history has the day. A restored wallet's
    old payments therefore get their day's price, not today's. Without
    either, the payment has no price and screens say so (see Part B). The
    daily histories are kept too (1,000 days, three currencies at most, the
    oldest asked dropped). A wallet file from before prices opens with them
    on, in dollars, from Kraken over the internet (serde defaults), as
    tested.
63. **Money is written one way whatever the phone's language**: the symbol
    before for the dollar and the euro ("$412.06", "€12.50"), otherwise the
    code after ("412.06 GBP"); a point before the cents, commas between
    thousands, always the cents. Typed money takes "12.5", "12,50" (a comma
    as the point, as XMR amounts), "$12.50" or "12.50 USD"; no thousands
    separators, no more decimals than the currency has. Money to XMR uses
    as few decimals (at least 4) as still come to the same cents, so a
    code asks for "0.1204", not "0.120409391932".
64. **Settings.** The "Theme" card became "Display": the three theme
    choices, then Currency (the provider's list, in a dialog; default USD)
    and "Show amounts in", XMR | USD. A new "Prices" card: "Show prices"
    (on by default; "Off: Fob never asks for a price") and "Price source"
    ("Kraken · over the internet", and the last request's error while it
    lasts), whose dialog lists "Kraken · over Tor" muted with "Not
    available yet"; Rust refuses it as well. All of it lives in the wallet
    file with the rest of the wallet's settings, so each wallet (each
    network) has its own; Rust writes the source's words.
65. **One segmented control (`Segmented`)** for every choice between XMR
    and a currency: a sunken track, the chosen option filled orange
    (#FF6600) with #111111 text, the others clear with muted text, as asked.
    No outline.
66. **Home's balance** shows what it's worth now under the XMR, or first
    with the XMR under it when amounts show in the currency first; tapping
    it swaps them, saving the same setting. Nothing extra shows without a
    current price (prices off, none asked within the hour, a failed
    request). An Activity row's second line on the right is the payment's
    recorded value (worth then); a receipt row keeps the receipt's total.
    A row without a price shows nothing there, no dash. Each piece of money
    not ready yet shows its value now beside the XMR. The price task runs
    while Fob is on screen and stops with it, so the background sync never
    asks for prices; pull to refresh asks for both blocks and the price.
