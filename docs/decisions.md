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

38. **Amounts drop trailing zeros everywhere**: "0.5", "0.653", "1", never
    "0.5000". Balances and lists are still cut (not rounded) to four
    decimals, as item 29; `amount::format` no longer pads.
39. **Activity is grouped by day in Rust (`activity::by_day`).** The app
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
40. **Pull to refresh calls `Wallet::sync_now`**: it wakes the sync loop
    from its 20 s wait (or its backoff after an error) and returns when a
    round ends caught up or failed, at most 20 s later. With no loop running
    it does one step itself.
41. **Dark mode.** Colour roles are a `FobColors` per theme behind a
    CompositionLocal; `Fob.ink` and the rest read the current one, so screens
    keep naming roles. The orange, the text on it and what's drawn over the
    camera are the same in both. QR codes are always dark on a white card.
    The choice ("Same as the phone", Light, Dark) is an Android preference
    beside the network, not in the wallet file: it is only how Fob looks,
    the same for every wallet, and is needed before any wallet opens.
    Dialogs use the card colour instead of Material's tinted default.
42. **Seed words never wrap**: two columns numbered down (1–8, 9–16; 13 and
    12 for 25 words), one column from a font scale of 1.6.
43. **"0.5 XMR ready to spend"** shows under the balance only while some of
    it can't be spent yet (`Balance::ready_text`).
44. **Instant updates over ZMQ (`zmq`, `zmtp`).** Monokulo's ZMTP 3.0
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
45. **Off by default; the node's own host only.** The setting (on or off,
    and the address, kept while off) lives with the node in the wallet
    file. Rust checks the address: `tcp://host:port` (or `host:port`), no
    user, path or query, and the host must be the node's. Fob offers
    `tcp://<node host>:18083` (mainnet) or `:38083` (stagenet) and connects
    nowhere until it's turned on. Changing to a node on another host turns
    instant updates off rather than following somewhere not chosen.
46. **What an announcement does.** `chain_main` wakes the sync loop at once
    through the same wake `sync_now` uses; connecting does too, for blocks
    missed meanwhile. `txpool_add` has the waiting loop look at the pool
    (once caught up), 2 s after the announcement so a burst is one look,
    and the wait carries on. Polling stays as the fallback: every 20 s as
    before, every 2 minutes while the publisher is heard.
47. **ZMQ status comes with the sync status**: Off, Connecting, Listening
    or Can't connect with a few words of why, and Settings' subtitle as
    Rust writes it ("host:18083 · listening"). A change is reported to the
    app like any other sync step. On but not syncing reads "connecting":
    it connects again when syncing starts. Turning the switch on with no
    address Rust accepts opens the address screen instead.

## Third round: money not ready, the address pattern, Receive

48. **Money not ready yet is shown per output, not per payment
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
49. **Where a piece came from**: "Received · 08:40" (when it arrived), or
    "Change · Corner Café" / "Change · 86Ux … 7kQa" when it is change from
    one of this wallet's own payments, named as Activity titles it; change
    from a spend found on the chain that this phone didn't send (a restored
    wallet) is just "Change".
50. **When it will be ready**: about 2 minutes a remaining block, counted
    from the last block scanned if it came within the last 2 minutes, else
    from now (the wallet is catching up, a block is late, or the phone's
    clock is behind the block's). "09:02" is the phone's time of day only,
    with no date (a 60-block reward can cross midnight; rare enough).
    "in 14 min", rounded up, at least "in 1 min", then "in 1 h 20 min".
    Pieces are listed soonest ready first. Home asks Rust again every 30 s
    while the card shows, so the times count down between blocks.
51. **The summary**: "1.253 XMR not ready yet", "3 pieces · all ready about
    09:07" ("1 piece · ready about …"), and progress weighted by amount,
    Σ(amount × confirmations ÷ needed) ÷ total, shown as a whole percent
    rounded down. The old "confirming" pill and `Balance::confirming` are
    gone.
52. **The card**: folded by default under the balance; a button with
    expanded/collapsed state for TalkBack; the bar and each ring report
    themselves as progress. Green is `Fob.green` (not the brighter
    `greenFill`), which keeps 3:1 against the line colour in light mode.
53. **A payment's own screen takes its status from Rust**
    (`activity::status`): Activity's words while it waits ("Sending",
    "On the way", "Confirming · 3 of 10", "Not ready · 3 of 10"), then
    "Confirmed · 24". The app's `settled` flag went with it.
54. **The address pattern (`pattern`)**: dots on a 5 × 5 grid, mirrored
    left to right, in one of eight colours, on a white badge; no words, and
    only Fob draws it. Shown beside the address on Receive, under the
    expanded QR, and on the confirm screen before sending, so payer and
    payee can compare at a glance. It is a quick check, not a proof: with
    8 × ~32,000 patterns a deliberately made look-alike address is cheap to
    find, so it catches mistakes and lazy swaps, and the confirm screen
    still shows the address.
55. **Its derivation**, exactly, so another app could draw the same:
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
56. **Drawn**: grid side 66% of the badge, centred, each dot 84% of its
    cell, so the corner dots sit inside the circle; palette by index
    #B34700, #1F7A45, #2F5AA8, #7A3FA0, #A3324A, #1D6F7A, #5B5F17,
    #3D3A8C. The badge stays white in dark mode, with a 1 dp ring in the
    line colour so it doesn't vanish on a white card.
57. **Receive, second version.** The code sits on a white card (20 dp
    padding, 14 dp corners, no icon in the middle, about 220 dp) with a
    round 32 dp button on its corner; it or the code opens the code full
    screen: white in both themes, the amount large with the note under it,
    the code at 320 dp with a 4-module quiet zone, the address pattern,
    and the window's brightness at full until it closes (the window's own
    setting, put back after; no permission needed). The 32 dp button is
    below the 48 dp touch size, as drawn, since the whole code is a target
    too.
58. **The address in full, in groups of five (`uri::address_groups`)**:
    19 groups for a 95-character address, an integrated one split the
    same way with its last group shorter; the first, middle (index
    count ÷ 2) and last are highlighted, in two new theme roles
    (`addressMark` #FBE3CC / #4A2A12, `onAddressMark` #7A2F00 / #FFC9A0),
    bold. The four-character grouping it replaces (`full`, `Mono`) is gone.
59. **A note for the payer** goes into the code as `tx_description`, the
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

60. **Provider and transport are chosen apart (`price`).** A provider is a
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
61. **Kraken (`price::kraken`).** Public REST, no key: `Ticker` for now (the
    last trade, `c[0]`), `OHLC` at 1440 minutes for history. Kraken lists
    XMR in two currencies of money, USD (`XMRUSD`, answered as `XXMRZUSD`)
    and EUR (`XMREUR`, `XXMRZEUR`), checked against `AssetPairs` on
    2026-10-31 (its other XMR pairs are USDC, USDT and XBT); the list is
    hard-coded and tested. A day's price is its volume-weighted average
    (nearer any moment of the day than the close), or the close for a day
    without trades. OHLC returns at most 720 days; older is "not
    available". One live test, ignored like the node tests, asks Kraken
    once each way; it passed on 2026-10-31.
62. **What a price request may say.** Nothing at all while "Show prices" is
    off. Only the pair, never anything of the wallet, with
    `User-Agent: Fob` and reqwest's bare defaults (no cookies). Never a
    price for a payment's moment: history is always the provider's whole
    window in one request (no `since`, which would give away the oldest day
    wanted), at most once a day, and only while some payment within the
    window lacks a price; days are looked up on the phone. The current
    price is asked at most every 5 minutes while Fob is on screen (a task
    started and stopped with the screen, woken at once by a price setting
    changing) and on pull to refresh, at most once a minute.
63. **Prices are whole numbers**: millionths of the currency per XMR, from
    the decimal text without going through floats. Values round half up to
    the cent. A current price older than an hour isn't shown or recorded.
64. **A payment's price is recorded with it in the wallet file**
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
65. **Money is written one way whatever the phone's language**: the symbol
    before for the dollar and the euro ("$412.06", "€12.50"), otherwise the
    code after ("412.06 GBP"); a point before the cents, commas between
    thousands, always the cents. Typed money takes "12.5", "12,50" (a comma
    as the point, as XMR amounts), "$12.50" or "12.50 USD"; no thousands
    separators, no more decimals than the currency has. Money to XMR uses
    as few decimals (at least 4) as still come to the same cents, so a
    code asks for "0.1204", not "0.120409391932".
66. **Settings.** The "Theme" card became "Display": the three theme
    choices, then Currency (the provider's list, in a dialog; default USD)
    and "Show amounts in", XMR | USD. A new "Prices" card: "Show prices"
    (on by default; "Off: Fob never asks for a price") and "Price source"
    ("Kraken · over the internet", and the last request's error while it
    lasts), whose dialog lists "Kraken · over Tor" muted with "Not
    available yet"; Rust refuses it as well. All of it lives in the wallet
    file with the rest of the wallet's settings, so each wallet (each
    network) has its own; Rust writes the source's words.
67. **One segmented control (`Segmented`)** for every choice between XMR
    and a currency: a sunken track, the chosen option filled orange
    (#FF6600) with #111111 text, the others clear with muted text, as asked.
    No outline.
68. **Home's balance** shows what it's worth now under the XMR, or first
    with the XMR under it when amounts show in the currency first; tapping
    it swaps them, saving the same setting. Nothing extra shows without a
    current price (prices off, none asked within the hour, a failed
    request). An Activity row's second line on the right is the payment's
    recorded value (worth then); a receipt row keeps the receipt's total.
    A row without a price shows nothing there, no dash. Each piece of money
    not ready yet shows its value now beside the XMR. The price task runs
    while Fob is on screen and stops with it, so the background sync never
    asks for prices; pull to refresh asks for both blocks and the price.
69. **Receive takes an amount in money.** With prices on and a current
    price, the field starts in the currency ("$" before what's typed) with
    a USD | XMR switch inside it at the right; under it Rust's "The code
    asks for 0.1204 XMR · 1 XMR = $166.10". Switching converts what's typed
    (Rust, at the current price). Without a price: XMR only, no switch, and
    an amount typed in money is cleared if the price goes. The code always
    asks in XMR (`tx_amount`, item 65's rounding); the expanded code shows
    "$20.00" large and "0.1204 XMR · note" under it. `receive_code` moved
    onto the wallet (it needs the price); the free function went.
70. **A payment's own screen (`activity::detail`, `Wallet::payment`)** for
    every row without a receipt: the signed amount with every digit and
    "XMR", "Today 08:40 · note" (sentence-case dates: "Yesterday", "27 Oct",
    "27 Oct 2025"); "Worth now" and "When received" / "When sent" side by
    side, each its value and rate or "Not available"; "Up $3.63 (4.6%)
    since then" in green, or "Down …" muted, when both are known in the
    same currency (tenths of a percent, rounded half up); then Status with
    a 4 dp green bar while under 10 confirmations, From ("Address 3 ·
    note", "Main address") or To (the short address), Pieces ("2 · 0.3 +
    0.2", largest first, "+ 2 more" past four; "Pieces spent" for a
    payment out), the fee of a payment out, and the transaction id cut to
    its ends with Copy. "Prices from Kraken." under it when a price shows.
    With prices off the two boxes don't show at all. Every word is Rust's.
    A receipt row still opens the receipt, now with the same boxes and
    details under it. The app's Activity record lost what only the old
    screen used (fee, to, time, confirmations, pending, status text).
71. **Notes stay with payments.** The note typed on Receive is kept for
    that address (by index) when the screen closes or a new address is
    taken, if not blank; a payment to the address shows it. A payment code's
    description is kept with the payment sent (`Sent::note`). Both are new
    optional fields in the wallet file. A note belongs to an address, not a
    payment: two payments to one address share it.
72. **Sending shows money too.** The confirm screen shows the amount's
    value now under it (not for a receipt, whose total already leads), and
    the fee and total each with theirs, at the current price; nothing when
    there is none.

## Onion addresses

73. **No `.onion` host is ever looked up.** Fob has no Tor transport, so a
    node or ZMQ publisher on an onion service would have been resolved
    through the phone's DNS, telling the resolver which onion service the
    wallet uses (step 2's finding). Rust now refuses one with "Fob can't
    reach .onion nodes yet" (`node::is_onion`: a host ending `.onion`, any
    case, with or without the root's dot): choosing it as the node
    (`set_node`, an Invalid error, before asking it anything), testing its
    speed, `on_network`, building a node transport at all (so an onion node
    already in a wallet file fails syncing with that message), a ZMQ
    address (`zmq::endpoint`), and connecting a ZMQ subscriber (`zmtp`,
    for an address saved before). The price transport refuses an onion
    price source the same way ("Fob can't reach .onion price sources yet").
    Each is tested without the network. When a Tor transport exists, these
    checks are where it plugs in.

## Fifth round: splitting up, loose ends

74. **Receive's note is cut to what a code holds (`uri::fit_note`)**: at
    most 120 characters and 360 bytes once percent-encoded (120 characters
    that each need encoding), cut at a character's end, so emoji and other
    scripts that take more room stop sooner (34 of "☕😀" alternating). The
    longest code Fob can then make, an integrated address, the longest
    amount and a full note, is QR version 18 or below at medium error
    correction (89 modules, about 2.5 dp each on Receive's 220 dp card),
    as a test checks for four kinds of note. The field asks Rust with
    each change (`fit_receive_note`) and shows "That's as long as a note
    can be" when what was typed was cut; `payment_uri` and `receive` cut as
    well, whatever reaches them.
75. **Activity's "Not ready" follows the not-ready card's rule.** Each row
    carries the confirmations it needs (`Activity::needed`): for a payment
    in, the most any of its outputs needs to be spent (`Owned::needed`:
    10, 60 for a mined reward, more for an output locked to a later
    block); 10 for a payment out and for one still in the pool. A reward
    at 24 confirmations reads "Not ready · 24 of 60" in Activity and on its
    own screen, whose bar is then confirmations ÷ 60, until it can be
    spent; no row says "Confirmed" while the card still lists its money.
76. **Split up pays one subaddress of this wallet (`split`,
    `Wallet::prepare_split`).** The pieces all go to one address kept for
    splits (`split_index` in the wallet file): the next unused index the
    first time a split is prepared, the same one after. Receive never hands
    it out (`receive_address` and `new_receive_address` step past it), and
    money found on it doesn't move Receive's index on to it. A restored
    wallet doesn't know it: its pieces are still found (the index is within
    the 50 watched past the last handed out) and Receive may then pass it;
    nothing is lost. Change goes to the main address as for any payment. On
    the chain a split looks like any 16-output payment; nothing names the
    address. It reuses the payment flow: the confirm screen (title "Split
    up", To "This wallet", the pieces listed by note, no address pattern
    since there's no one to compare with), swipe and fingerprint.
77. **At most 15 pieces**: Monero's 16 outputs a transaction, one of them
    the change, which monero-oxide always adds (when nothing is left over it
    is a zero-amount change output, which also hides that it was exact).
    Rust refuses a 16th piece ("15 pieces is the most one split can make")
    and a piece that with the estimated fee wouldn't fit what's ready to
    spend ("Not enough ready to spend for another $20 piece"); the screen
    shows the words for 3 s. Ready to spend means the outputs `Balance`
    counts as unlocked, spent largest first as `prepare` chooses them.
78. **Notes.** With prices on and a current price: 5, 10, 20, 50, 100 and
    200 of the currency ("$20", "20 GBP" for one without a symbol), each set
    in XMR at today's price with as few decimals as still come to the same
    cents (`price::to_atomic`), so the pieces are XMR and their value moves
    from then on, as the footer says. Without a price: 0.01, 0.05, 0.1,
    0.25, 0.5 and 1 XMR: the same six-tile grid, and roughly the same
    spread as the money notes at today's prices.
79. **The fee shown is an estimate**: "Network fee about $0.03", from a
    weight formula (`split::weight`: CLSAG inputs, an additional key per
    output past two, a Bulletproof+ padded to a power of two with Monero's
    clawback) at the node's fee rate, asked when the screen opens and kept
    (also by every `prepare`), or Monero's usual 20,000 piconero per unit
    of weight until then. The confirm screen shows the real fee. Under a
    cent reads "less than $0.01".
80. **The suggestion (`split::suggest`)**, deterministic: from 3 or more
    payments out in the last 30 days (splits not counted), each payment
    counts for the smallest note that covers it (the largest note for one
    bigger than all); pieces are then added one at a time to the note
    furthest below its share (smaller notes first on a tie), skipping a
    note once another of it doesn't fit, up to 15 pieces. The words name
    the 20th to 80th percentile of the payments, whole units rounded
    ("most payments were $5–$25", "about 0.1 XMR" when they agree). With
    fewer payments: 12 pieces, 4 each of three neighbouring notes, the
    largest three whose 12 fit, else as many as fit of the smallest three,
    with "Not enough payments yet to learn from, so here's an even mix.
    Change it to suit." Either way the rest stays as one piece.
81. **The screen** keeps only the counts; every word, tile label, the
    accessibility labels ("Add a $20 piece, 4 so far", "Remove a $20
    piece"), the bar (each piece's share of the width and the square root
    of its value for its height, what's left over drawn in the track
    colour) and the button's words come from Rust (`Wallet::split_view`).
    Entries: Activity's header ("Split up", orange text) and, while the
    not-ready card is open, "Split up your balance" under its pieces; only
    in a wallet that can spend. Clear empties the counts.
82. **A split in Activity**: titled "Split up", its line "12 pieces · 10:15"
    (with "Confirming · 3 of 10" first while it confirms), and its amount
    the fee, since only the fee left the wallet; the pieces' total is in
    the wallet file (`Sent::pieces`, `Sent::amount`). While it isn't mined
    the balance counts everything coming back (pieces and change), and the
    not-ready card shows it as "Change · Split up". Its own screen says To
    "This wallet". No real split was sent in this round: there's no funded
    stagenet wallet among the tests.
83. **A sent payment keeps each output coming back (`Sent::back`)**: a
    split's pieces, then the change, recorded when it's sent. Before it's
    mined the not-ready card lists them one by one, so a split of six reads
    "7 pieces" from the start instead of "1 piece" until it's mined (found
    on the phone, 2026-10-31). Files from before keep `change` as one piece.

## Round 3: Home, coins, the amount field

84. **Home's balance is one line** (Round 4 › "Home · balance C2"): what
    "Show amounts in" puts first, large, on the left (the XMR with "XMR"
    beside it, or the money), the other at the line's end; tapping still
    swaps them. The large figure shrinks to fit (down to 24 sp) rather than
    wrap, for big balances on narrow phones. The "Balance" label goes, but
    "Balance so far" stays above the line while Fob is catching up, since
    the figure isn't the whole story yet.
85. **Ready and not yet, folded** (Rust: `not_ready::summarise`, now given
    what's ready): only while some money isn't ready, a card reading
    "1.8592 ready · 0.6221 not yet", the time the last of it should be
    ready, and a 6 dp bar: the ready part in ink, the not-ready part on the
    track, each as wide as its share, the not-ready part filled green by
    the weighted progress. Opened (animated, the chevron turning): "Ready
    to spend", "Not ready · 3 coins" and "All of it", each in XMR and money,
    then "All ready about 09:02 · in 14 min". The old card's title, line
    and percent, and `Balance::ready_text` ("0.5 XMR ready to spend" under
    the balance), went with it. TalkBack reads the folded card as one
    sentence from Rust.
86. **Home's buttons are Receive (outline) and Send (orange)**. Scan to pay
    and both Split up links (Activity's header, the old card) are gone; the
    old card's coin by coin list moves to the Coins screen (step 2). The
    Scan to pay camera screen stays in the code, reached by nothing until
    step 4 replaces it with Send's camera.
87. **The Coins screen** (Round 4 › "Coins A"), from Activity's "Coins"
    chip (a sunken pill, 40 dp drawn in a 48 dp touch area), for every
    wallet, view-only ones too; its orange Split up button only in a wallet
    that can spend. Rust (`not_ready::coins`, `Wallet::coins`) writes it
    all: the intro ("Your 2.4813 XMR is held as 8 coins. A payment uses
    whole coins; what's left comes back as a new coin, not ready for about
    20 minutes.", the minutes from the 10-block lock; "No coins yet. Each
    payment to this wallet arrives as a coin." when there are none), the
    bar from Home, "Ready · 5 coins" with "1.8592 · $308.70" and each coin
    largest first, and "Not ready · 3 coins" with "all by 09:02" and each
    soonest first ("3 of 10" from Rust too). Its ready coins and Home's
    "Ready to spend" are the same outputs `Balance` counts as unlocked
    (`Wallet::coin_pieces` sorts every coin into the two). It refreshes with
    the wallet and each half minute while some coins wait.
88. **Where a coin came from**, one rule for ready and not ready:
    "Received · 08:40" today, "Received · 22 Oct" before (the board's
    "Received 22 Oct" given the same " · " as every other line, and a year
    once it's another year's), "Split up · 08:31" for a split's coins and
    its change (it was "Change · Split up"), "Change · Corner Café" for a
    payment's change, "Change" when the payment isn't known.
89. **One amount field, toggle B** (Round 3 › "Toggle B"; Shared ›
    "Amount field"): the sign in front, what's typed (Manrope 22 bold), an
    orange chip naming the unit with two arrows that turn half a turn in
    260 ms per tap (always the same way round), and a muted line under it.
    Rust (`price::amount_field`, `Wallet::amount_field`) writes the sign
    ("$", "€", "ɱ" for XMR; none for a currency without a symbol, which the
    chip already names), the unit, the line ("≈ 0.1204 XMR · 1 XMR =
    $166.10", "≈ $20.00 · 1 XMR = $166.10", the rate alone while empty,
    "That isn't an amount"), the field's TalkBack label ("Amount in US
    dollars", "Amount in euros", "Amount in CHF") and the chip's ("Amount in
    XMR. Switch to US dollars"); tapping converts what's typed in Rust, as
    before (`convert_amount`). Without a price the field takes XMR with ɱ
    and has no chip. Receive uses it in place of its in-field segmented
    control; its "The code asks for 0.1204 XMR" line (`ReceiveCode::asks`)
    gave way to the field's line, which is the same conversion, and
    `receive_currency` to `amount_field`. Settings' "Show amounts in" keeps
    its segmented control (decision 3 of the plan).
90. **ɱ isn't in Manrope**: the bundled `manrope.ttf` has no U+0271 (its
    cmap's 0x0260–0x027F word is empty; checked with fc-query), so the XMR
    sign is drawn in the system font (Roboto, which has it) at the same
    bold weight and size; "$" and "€" stay in Manrope.
91. **Send keeps its XMR-only amount field until step 4**: switching it to
    toggle B means `prepare` taking an amount in money (or converting
    first, with the price moving between the two), and the Send screen is
    redrawn in step 4 anyway, so it moves to the new field there.

## Round 3: Send with the camera

92. **Send is one screen, the camera under a sheet** (Round 3 › "Send B"):
    Home's Send opens it, and a `monero:` link opens it too, its code read
    as if scanned. The old Scan to pay screen and the old Send form are
    gone (`ScanPayScreen`, `Screen.ScanPay`), and with them ffi's
    `is_amount`, which the field's `sendable` (decision 96) replaced.
    Folded, the sheet holds "Paste an address", the people paid before and
    the ready line; pulled up (the handle dragged 40 dp, or tapped), the
    form: To, Amount (toggle B), Note, the ready line and an orange Review
    to the confirm screen, which is unchanged (pattern, swipe, fingerprint).
    Back folds the sheet first, then closes Send. The camera stays full
    screen behind the sheet and keeps scanning, its viewfinder drawn small
    (100 dp) above the form, or not at all where the screen is under
    560 dp tall (landscape); folded, the sheet takes at most 62% of the
    height so a landscape phone keeps a strip of camera to aim with.
93. **What a scan, a paste or a link does**: a wallet code, or anything
    that isn't a payment code, says so (over the camera folded, under To
    pulled up); an address, or a payment code without an amount, fills To
    (and Note from its `tx_description`) and pulls the sheet up; a payment
    code with an amount goes straight to the confirm screen with its
    receipt and description. If that payment can't be prepared (not enough
    ready, no node), the form opens filled, the amount in XMR, with the
    reason. Changing the address or that amount drops the code's receipt,
    which was for that payment only. The camera reads a code many times a
    second, so each text is taken once until To is edited. A whole
    `monero:` code typed or pasted into To is read as a code.
94. **The clipboard is read only when Paste is tapped** (the sheet's row
    and the To field's button), never to preview it, so Android shows no
    paste notice before the tap; an empty clipboard says "Nothing to
    paste."
95. **Paid before, until contacts** (`activity::paid_before`,
    `Wallet::paid_before`): where the board has CONTACTS with "All", the
    sheet shows "PAID BEFORE" (no "All": nothing to open yet) until steps 5
    and 6: the last 3 addresses paid, each once, newest first, splits left
    out (they pay this wallet). Each row: the address's pattern, the shop
    on the last payment's receipt or the address cut to its ends ("86Ux …
    7kQa", as in Activity), and a line about the last payment: its note
    and day ("Lunch money · 27 Oct"), or its day and amount ("Today ·
    $24.50": the receipt's total, else its worth when sent, else "0.1421
    XMR"). Hidden when nothing was sent. Tapping fills To and pulls the
    sheet up. Step 6 swaps contacts in at the marked spot in `Send.kt`.
96. **Send takes an amount in money** (`Wallet::typed_amount`,
    `Wallet::prepare_typed`; ffi `prepare(to, amount, in_fiat, …)`): the
    text as typed and the field's unit go to Rust, which converts money at
    the current price with the rounding the field's line showed ("≈ 0.1204
    XMR"), so what's reviewed is what was shown unless the price moved in
    between (the confirm screen's XMR is then the truth). Receive's code
    uses the same conversion. The field opens in money while there's a
    price, as Receive's does. `AmountField::sendable` says whether what's
    typed comes to more than nothing, for Review.
97. **The note is the sent payment's note**: typed on Send (placeholder
    "What it's for, kept on this phone"), or prefilled from a code's
    `tx_description`, and kept in `Sent::note`, the field the description
    already used, so it shows on the payment's screen after the time and on
    the confirm screen under the amount. It never leaves the phone.
98. **To shows "84Bv9 … 99fQ2" while not being edited** (`uri::short`,
    ffi `short_address`): the first and last five, as the board, with the
    pattern in front once it's an address of this network (Rust's
    `parse_payment` decides); tapping edits the whole address underneath.
99. **The ready line** (`Wallet::ready_line`, `Balance::ready_line`):
    "1.8592 XMR ready to spend · $308.70", the money only with a current
    price; under the folded sheet's rows and under the form.
100. **Safe areas over the camera**: the top bar sits below the status bar
    and clear of a cutout (`safeDrawing`, top and sides), the sheet's
    content above the gesture bar and the keyboard and clear of the sides;
    the camera and the sheet's paper still reach the edges. Restore's
    "Scan a wallet code" screen, which had the same 32 dp guess at the
    status bar, now uses the safe area too.
101. **Torch**: shown only once the camera is open and has a light, as a
    switch ("Torch", on or off for TalkBack); on, its circle fills with the
    camera's card colour and the lamp turns orange. It goes off with the
    camera when Send closes.

## Round 3: contacts

102. **A contact is kept in the wallet file** (`contacts::Book`, the
    state's `contacts`, read as none from older files): an id (never used
    again after a delete), a name (1–40 characters once trimmed, control
    characters dropped), an optional note (up to 80 characters; longer is
    refused, not cut), an optional photo (a WebP, checked by its `RIFF` …
    `WEBP` header, of at most 1536 bytes, kept as hex in the JSON), an
    optional send address and the addresses it was also paid at, and when
    it was made. A send address is checked for the wallet's network; a
    pasted or scanned `monero:` code gives its address. Two contacts may
    share a send address, without a word. A change Rust refuses leaves the
    file as it was.
103. **Send address and also paid at stay apart**: an address from a code
    that only named someone goes to `also_paid_at` and is never used to pay
    them; making one of those the send address (scanned or pasted) moves
    it out of `also_paid_at`. Payments to any of a contact's addresses are
    theirs: in Activity (the title, and "To" on the payment's screen, are
    the contact's name, before a receipt's shop), on their card ("Paid to
    Sam", newest first) and in the lines about the last payment. Splits
    are no one's. Deleting a contact leaves every payment, shown by its
    address again.
104. **The lines** (`contacts::list`, `on_send`, `unsaved`, `card`):
    Contacts lists by name (any case) and its search looks in names and
    notes; each line is "Lunch money · paid 27 Oct", "Paid today", the note
    alone, or "Not paid yet". Send's sheet takes contacts with a send
    address, the most recently paid first (never paid ones after, by
    name), at most 3, each line the last payment's as Paid before had it
    ("Lunch · 27 Oct", "Today · $24.50"), else the note or "Not paid yet".
    "Paid, not saved" lists every address paid that's no contact's, each
    once, newest first, as "84Bv … 9fQ2" and "Paid 25 Oct · $33.21". A
    card shows addresses as their first, middle and last five ("86Uxm …
    3EkYw … e7kQa", `contacts::three_part`), each paid-at address with "Not
    their send address · paid once, 25 Oct" ("paid 3 times, last …", "not
    paid yet").
105. **Back from Confirm returns to the form as filled** (`SendDraft`):
    Review puts the form's state (To, the amount as typed and its unit,
    the note, the code it came from) on the back stack as
    `Screen.Send(draft)` under Confirm, which opens pulled up and filled,
    and doesn't take the code it came from again while the camera still
    sees it. Once sent, both go from the stack, so Back from Paid is Home.
106. **Where contacts are** (Round 4 › "Contacts"): Send's sheet's "All"
    and a "Contacts" row at the top of Settings' Wallet card (step 11 may
    move it). The search shows once there's a contact; saved contacts
    (avatar, name, Rust's line, chevron), then "Paid, not saved" with a
    Save on each, which opens New contact with that address as the send
    address. Empty: "No contacts yet. Tap + to add someone you pay." (with
    ", or save someone you paid below." when there's something below); a
    search that finds nothing says so. The board's footnote lost its last
    clause ("a received payment can be named by hand"): Fob has no way to
    do that yet.
107. **A contact's card** (Round 5 › "Sam"): Send opens Send pulled up
    with To filled by the send address (and Send's camera when there's
    none); Request opens Receive as it is until step 8 (marked in
    `Contacts.kt`, as is the requests section's spot). The send address
    row's Change (Add without one) opens the scanner, and Use it keeps the
    address at once. "Also paid at" shows only when there's one. Each
    payment in "Paid to Sam" opens its own screen.
108. **The edit card** (Round 5 › "Edit"): Save, an ink pill in the top
    bar, is off until an edited contact has loaded; a new contact then
    opens its card, an edited one goes back to it. Rust's refusal shows
    above Delete contact. The send address is scanned (full-screen camera,
    title and close below the status bar, the found sheet above the
    gesture bar, `SendAddressScanner`) or pasted; a payment code gives its
    address. Pasting over an address asks first with the same sheet
    (Cancel / Use it). There's no way to empty a send address once set:
    the board has none, and scanning or pasting replaces it. Delete asks
    on a sheet (Delete in red, Keep), then goes back past the card.
109. **The photo** (Round 5 › "Contact photo"): Take a photo uses the
    system camera's preview picture (`TakePicturePreview`: no file, no
    FileProvider, only the camera permission Fob already asks for, which
    the system camera then needs granted); Choose from your photos uses
    Android's Photo Picker (no storage permission). The picture is cropped
    to a centred square, scaled to 96, 72 then 64 pixels and encoded with
    Android's WebP encoder at quality 80 down to 20 in steps of 10 until
    Rust says it fits (`contact_photo_fits`); one that never fits is
    refused with a line. Remove photo shows only with a photo.
110. **Avatars wherever a contact is**: the photo in a circle or Rust's
    initial on the sunken colour (`Avatar`), on Contacts, the card, the
    edit card and Send's sheet, where contacts take the place of the
    board's address patterns; patterns stay for addresses. Send's sheet
    shows CONTACTS with "All" once there's any contact (a line asks for a
    send address when none has one), PAID BEFORE until then. Two theme
    roles came with the sheets: `scrim` and `onRed`.
111. **The name in a payment code** (`recipient_name`, read by
    `uri::parse_payment` with `contacts::clean_name`): percent-decoded,
    trimmed, control characters and the marks that turn text round
    (U+200E/F, U+202A–E, U+2066–9) taken out, cut to 40 characters; one
    that's left empty is none. Typed names and notes lose the same
    characters. It goes with the payment (`Sent::recipient_name`, none in
    older files) only while the form is still the code's: changing the
    address drops it with the receipt.
112. **After paying, Paid offers to save who was paid**
    (`contacts::offer`, `Wallet::save_offer`): nothing for a split or an
    address that's already a contact's (send address or paid at). A code
    whose name matches a contact's exactly, any case (the oldest of
    several), gives an outline "Add to Sam" with Sam's avatar (the name
    ellipsised), which keeps the address in Sam's `also_paid_at` and opens
    Sam's card, and "A different Sam? New contact" under it. A name that's
    nobody's gives "Save as a contact" (person-plus, 20 dp, stroke 2),
    opening New contact with the name and the payment's note, the address
    kept as one they were paid at and the send address empty. Without a
    name, "Save as a contact" fills the note and makes the address the
    send address. A named code adds "to a code named “Sam”" under the
    status and, under the buttons, the board's whole footnote: "The name
    comes from their code, not checked by Fob. The address is kept with the
    contact as one they were paid at, not as their send address." (the
    brief quoted its first sentence; the second says what the buttons do).
113. **"Paid, not saved" saves by the same rule** (`Wallet::new_contact_for`):
    an address ever paid with a named code opens New contact with that
    name and the address as paid at; otherwise as its send address. New
    contact says what happens to such an address under the send address
    ("Paid at 84Bv9 … 2kLmQ … 99fQ2 from their code: kept with them, not as
    their send address.", `contacts::paid_at_note`).
