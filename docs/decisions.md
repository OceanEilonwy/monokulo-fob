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

## Round 3: requests

114. **A request is kept in the wallet file** (`requests::Book`, the
    state's `requests`, none in older files): an id (never used again), its
    subaddress and address, the contact it's for (or none), the note
    (control characters and turning marks out, cut to what a code holds, as
    Receive's), what it asked (`Asked`: the text as typed, its unit, the
    XMR it came to then, and the money written out when typed in money; none
    without an amount), and when it was made. Nothing about it changes
    after. Deleting marks it with the time instead of removing it, so its
    address stays known: payments from before keep their name.
115. **Made on Share or Copy only** (`Wallet::make_request`; decision 2 of
    the plan): Receive opens on its own address, as before, and its code is
    that address's until Share or Copy, when the request is made and the
    code switches to it. Share or Copy again with nothing changed (the same
    contact, amount, unit and note) uses that request again rather than
    making another; changing any of them puts Receive's own code back until
    the next Share or Copy. Copy copies the request's address, as Copy
    copied the address before. A note typed and then made into a request
    isn't also kept for Receive's own address.
116. **One counter hands out subaddresses** (`Wallet::hand_out`):
    `receive_index` is the highest given to anything, and Receive's own
    address is kept apart (`receive_own`; in an older file, the last one
    handed out, as it showed). A request takes the next one, skipping the
    address splits pay to; Receive's own is never a request's or the
    split's, and New address takes the next as well, so no address is given
    twice. A deleted request's address is below the counter, so it's
    scanned for as long as the wallet is.
117. **A request shows only what came** (`requests::received`): the sum of
    every payment to its address, mined or still in the pool (the pool now
    keeps the subaddress paid), else "Waiting"; never what's left, and it
    never expires. In the money it asked in when it was asked in money and
    every payment has a price recorded in that currency ("received
    $83.03"), else in XMR ("received 0.5 XMR").
118. **Activity** (Round 5 › "Activity: requests and payments, separate
    lines"; `activity::by_day` now takes `requests::activity_rows`): each
    request not deleted is a line among the payments by its time (payments
    not yet mined stay on top): the request code icon on the sunken disc,
    "Request to Sam" or "Request", "Concert tickets · received $83.03 ·
    08:30" or "No name · Bike · waiting · 08:12", and on the right the
    amount asked, grey, with "asked" (nothing for any amount). A payment
    in now starts its line with what it was for, Receive's note as well as
    a request's ("Pizza · 19:05", the status after it in lower case), and
    is titled "From Sam" while its request is for a contact; its own screen
    says "Sam · Address 9 · Concert tickets" under From.
119. **Deleting a request** (asks first, Round 5's words): it leaves
    Activity and the card; its address still receives, what comes after
    shows as "Received" with the request's note (what it was for, not who),
    and what came before still says "From Sam" and stays on Sam's card. A
    request for a contact since deleted reads "Request" and "No name".
120. **The request screen** (Round 5 › "A request"): the amount asked large
    ("Any amount" without one), "asked of Sam · Concert tickets" ("asked,
    no name", "for Sam" without an amount), "Today 08:30 · 0.4817 XMR at the
    time" (the XMR only for an amount asked in money, where it's news),
    Received, the address with its pattern, "Payments to it" as Activity
    has them but with their day ("Today 08:40 · not ready · 3 of 10"), or
    "Nothing paid to it yet.", Show the code again (the code as made, from
    the XMR it came to then, never converted again), and Delete request.
    Without a name, the confirm drops "and Sam's card" and "shown without a
    name".
121. **A contact's card lists their requests** between the send address and
    Also paid at, only when there's one: "Requests · Sam pays you", "3
    addresses", each newest first with its pattern, note (else "Request"),
    "Address 9 · today 08:30" ("yesterday", "21 Oct"), "Received $83.03"
    in green or "Waiting" muted, and "asked $80.00"; tapping opens it. What
    came from them joins the payments to them ("From Sam", "Concert tickets
    · Today", "+0.5" in green), and the section is then "Payments with Sam"
    rather than "Paid to Sam".
122. **Share asks who it's for** (Round 5 › "Share asks who it's for"):
    every contact by name, each with Rust's line ("2 requests paid, 1
    waiting", "1 request waiting", "No requests yet", `requests::summary`),
    scrolling when there are many; New contact opens the edit card and,
    saved, comes back to Receive filled as it was with them chosen, and
    shares; Share without a name forgets any choice. Chosen, a chip over
    the code shows their avatar and name (as wide as the name, ellipsised
    when long) and opens the sheet again, and Share reads "Share with Sam"
    and shares at once. A contact's Request opens Receive with them chosen;
    nothing is made until Share or Copy.
123. **A name in requests' codes** (`uri::payment_uri` takes a name, put in
    as `recipient_name` after cleaning it as a contact's name): only a
    request's code would carry it, since what's shared is always a request;
    none until step 9's profile and "Name in payment requests" fill
    `Wallet::name_in_requests` (marked), and Receive's line saying so has
    its spot marked in `Pay.kt`.

## Round 3: your profile

124. **The profile is kept in the wallet file** (`profile::Book`, the
    state's `profile`, none in older files): a name (1–40 characters,
    cleaned as a contact's; "Your profile needs a name."), an optional
    photo (the contact-photo pipeline, now `ui/Photo.kt`: the same WebP of
    at most 1536 bytes, hex in the JSON) and "Name in payment requests",
    on for a first profile and left as set by later edits. There's no way
    to delete a profile: Edit changes it, Remove photo drops the photo.
125. **Requests carry the name while the switch is on**
    (`Wallet::name_in_requests` now reads the profile): the code made on
    Share or Copy, and "Show the code again", put it in as
    `recipient_name`. Codes are built when shown and the name isn't kept
    with a request, so turning the switch off takes it out of every code
    shown after, a request's made before included. Receive says so under
    the note with a person icon, "The code names you as “Henry” · change
    in Settings" (`profile::receive_line`); tapping it opens Your profile,
    where the switch is.
126. **No fixed profile address: every share is one**
    (`Wallet::share_profile`, decision 6 of the plan): pressing Share my
    profile takes the next subaddress from the one counter (`hand_out`, as
    requests do, so it's never Receive's, a request's or the split's) and
    keeps the share (id, subaddress, address, kind, when) before its code
    shows. Receive's own address is never a share's.
127. **A share's kind** (`ShareKind`, kept by name: `code_shown`,
    `link_copied`, `link_shared`): every share starts as Code shown; Copy
    link or Share on its screen makes it Link copied or Link shared, the
    last pressed; nothing makes it Code shown again. Step 10 adds a
    touched-phones kind, with whom, beside these (serde's external tags,
    so files written now read the same); `ShareRow` and the share screen
    mark where.
128. **The profile code** (`uri::profile_uri`, `parse_profile`):
    `monero:<share address>?recipient_name=<name>&recipient_avatar=<photo>`,
    the name percent-encoded as a request's, the photo URL-safe base64
    without padding (padding is read too). Other wallets see an address
    with a name. It's read as a profile only with a name and nothing to
    pay (no amount, note or receipt); a photo that isn't base64
    ("damaged"), is over 1536 bytes (measured before decoding) or isn't a
    WebP refuses the whole profile. The largest (95-character subaddress,
    a name of 120 bytes, decision 166, and a 1536-byte photo) is 2544
    bytes.
129. **Dense codes at low error correction** (`qr::encode`): a code that
    would be past version 20 at medium is made at low instead, a lower
    version; a usual payment or request code stays at medium (corrected
    in decision 166: a request with a long name and note can pass version
    20 and go to low). Version 40
    at low holds 2953 bytes, so every profile encodes (tested); a usual
    one (a short name, a photo near the budget) is version 33 or below.
    `QrImage` now draws each module a whole number of pixels, centred, so
    a dense code stays crisp (every code gains from it).
130. **A share's code again shows the profile as it is now**: the address
    is the share's, the name and photo today's; the code isn't kept.
131. **Activity**: a share is its own line among requests and payments,
    "Shared your profile", "A new address for this share · 09:15", the
    person icon on the sunken disc, no amount; tapping it shows that
    share's code again. A payment to a share's address is "Via your
    profile" with "Shared 09:15 · not ready · 1 of 10 · 09:31" (the
    share's time when made today, its date before), and its own screen
    says "Address 12 · via your profile" under From.
132. **Your profile** (Round 6 › "Your profile: every share listed"):
    avatar 80, name, Edit (the onboarding editor with a Back arrow and
    Save), orange Share my profile, the switch card ("Requests carry your
    name; only this profile code carries the photo"), and Your shares,
    newest first: Code shown (code icon) or Link copied / Link shared
    (link icon), "Today 09:15" ("Yesterday 18:02", "28 Oct"), the
    address's pattern, and what came, "+$4.98" in green (in the money
    prices are shown in when every payment has a price, else "+0.03 XMR")
    or "nothing yet". Tapping one shows its code again. With none, a line
    says what a share is. Settings opens it from a row at the top of the
    Wallet card (avatar 40 and "Henry · name and photo, share as a code",
    or the person icon and "Set up your profile", which opens the editor;
    saved, it opens Your profile). Step 11 may move the row.
133. **Share my profile** (Round 6 › "Share my profile"): white in both
    themes with the screen at full brightness (the expanded code's, now
    `FullBrightness`), Close, the avatar 52 with the name and "Scan with
    Fob to add me", the code as wide as the screen less 8 dp a side (it
    keeps its own 4-module quiet zone), "A new address for this share,
    never used again.", then Copy link (the code, as text) and Share (the
    system share sheet). The "Or touch phones" strip is left out until
    step 10; its spot is marked.
134. **Make it yours, after a new wallet and after a restore**: the seed
    screen's "I've written them down" goes on to it as "Step 3 of 3 ·
    optional"; a restored wallet gets it too, as "Last step · optional"
    (restoring takes two to four screens), since a restored file has no
    profile and moving a wallet is when people set one up. Skip goes
    Home; Continue saves (Rust's refusal shows above it) and opens Your
    profile. The photo sheet is the contact's, its footnote saying the
    photo leaves the phone only in the profile's code.
135. **Scanning a profile** (Round 6 › "Another Fob scans the code"):
    from Send's camera, a paste or a link (Send's sheet gives way to the
    profile's), and from a scan button on Contacts' top bar (a
    full-screen camera, "Scan a profile", which says why a code isn't
    one). The sheet: the photo at 88, the name, the address's pattern and
    three parts, "A Fob profile. The name and photo are Henry's own, not
    checked by Fob.", Pay Henry (outline: Send's form with To filled and
    the code's name kept with the payment, so Paid offers to save them as
    decision 112 says) and Add contact (orange: name, photo, the address
    as send address; opens the card). A code with a name and nothing else
    (a bare named address from any wallet) shows as a profile too: Pay
    does what such a code always did. A profile whose photo is refused
    is, on Send, a named payment code. Back or the handle closes the
    sheet; the camera doesn't take the same code again until another.
136. **Never changes a contact without asking** (decision 5,
    `profile::scanned`): a contact owning the address (send address, else
    paid at) or, failing that, the oldest with the same name (any case)
    is a clash. Add contact then shows why ("You have a contact named
    Henry.", "This address is Sam's send address.", "Sam was paid at this
    address."), each change as now → new (the name; the photo, drawn as
    both avatars; the send address in three parts), a line asking
    ("Update Sam? Their name and send address change to the profile's."),
    then Update Sam, Keep as is (back to the profile) and Add as new
    contact. Update takes the name, the photo (a profile without one
    leaves theirs) and the address as send address (moved out of paid
    at); the note stays. When nothing would change it says so and offers
    Open Sam instead.

## Round 3: touch phones

137. **Touch phones is card emulation and reader mode, taking turns**
    (Round 6's note): Android has no phone-to-phone NFC since Beam went,
    so while Share my profile is in front with NFC on, each phone answers
    as a card (`SwapService`, a `HostApduService`) and switches reader mode
    on and off at random 150–450 ms (a reading phone isn't a card), never
    mid-exchange (`busy`, which lets go after a second of quiet). Two phones
    on that screen settle into one of each within a touch. Fob's service
    is preferred while the screen is in front (`setPreferredService`).
138. **The application ID is F0 46 4F 42 53 57 41 50** (F0, proprietary,
    then "FOBSWAP"; `swap::AID`), in `res/xml/apduservice.xml` as category
    other with `requireDeviceUnlock`; a Rust test checks the file says the
    same.
139. **The protocol is all in Rust** (`swap.rs`; the app moves bytes):
    select, then exchanges `80 E0 <index>` each carrying the reader's chunk
    `index` and answered with the card's chunk `index`, at most 240 bytes
    each way in short APDUs (extended ones aren't on every phone), then
    `80 E2` done. Each profile code travels as a frame: version 1, its
    length (two bytes, at most 3000; the largest code is 2544), the first
    four bytes of its SHA3-256 (already a dependency), the code. Indexes
    run with no gaps. A frame cut short, longer than it says or the cap, of
    another version, or not matching its checksum ends the touch on both
    phones with nothing taken; so does a code `parse_profile` refuses (no
    name, another network, a bad photo). Touching again starts over.
140. **Both end with the other's, or the card knows the reader's first**:
    the reader takes the card's profile only once the card answers done;
    the card takes the reader's on done. If the link drops after done
    reaches the card but before its answer comes back, the card has the
    reader's and the reader hasn't the card's; nothing can close that gap
    over one link, and touching again (a new session on each side) fixes it.
141. **Only while Share my profile is in front**: the card service answers
    "not available" (6A82) to anything while no swap is set, which is
    whenever the screen isn't resumed (`LifecycleResumeEffect`), so a
    locked phone, a phone in a pocket or Fob in the background gives
    nothing. A swap that's over answers the same.
142. **The swap is on the share's own address** (`start_swap(id)`): the
    swap begins as Share my profile opens with NFC on (not on the strip:
    the board's line says being on the screen is enough), on the address
    the screen's share was given (decision 6's one per opening), so an
    opening hands out one subaddress however the profile goes out. When
    it's over (`end_swap`, on success or as the screen goes) the share is
    kept as "Touched phones with Sam" when theirs came, "Touched phones"
    when ours went out whole but theirs didn't (a payment to that address
    still comes "Via your profile"), and stays "Code shown" otherwise; a
    name, once there, stays. (First built with a second new address per
    opening for the swap; the emulator pass changed it.) The kind is
    `touched_phones` with the name, by name in the file beside the
    others; Copy link on its code again doesn't change it, since who it
    went to is what it's listed by.
143. **The screens** (Round 6 › "Touch phones", "Swapped"): the strip "Or
    touch phones · Both on this screen, backs together: you swap" (the
    phone with waves in orange) sits between the code and its line, only on
    a phone with NFC and card emulation; with NFC off it says so and opens
    NFC's settings, and the swap starts on coming back. The strip and the
    words around it keep the white screen's fixed colours in both themes
    (`qrStrip`, `qrAccent`, the light theme's warm grey and orange). Touch
    phones is on the theme's paper (rings, your photo, the board's words,
    why the last touch took nothing, "No NFC? Scan each other's code
    instead." going back to the code). Swapped has both photos with the
    tick, Rust's "Swapped with Sam" and "Sam got your profile, and you got
    Sam's.", their name, pattern and address, Not now (back to Your
    profile) and Add contact: straight to the new card, or, when it would
    change a contact, the asking sheet a scanned profile shows (decision
    136), with Pay there too. A haptic confirm on success. Your shares
    lists it as "Touched phones with Sam" with the phone icon; Activity's
    line for it stays "Shared your profile".
144. **Emulators have no NFC**: the strip hides there, and the protocol's
    proof is its tests (two peers in memory: whole swaps of the largest
    and smallest codes both ways, cut short, garbled each way, oversize,
    another version, chunks out of order, done refused, the link dropping
    midway and just before done, codes that aren't profiles, commands in
    the wrong state; two wallets swapping through sessions). The two-phone
    checks are in the checklist.

## Round 3: Settings

145. **Settings follows the board's sections** (Round 2 · Money ›
    "Settings: display, prices, node"), in this order: Wallet, Display,
    Prices, Syncing, then Network, Receipts and This phone, which the board
    doesn't show and stay as they were. Rows run without dividers inside a
    card, as on the board.
146. **The Wallet card is settled** (decisions 106 and 132 left it open):
    Your profile (avatar 40, "Henry · name and photo, share as a code", or
    the person icon and "Set up your profile"), Contacts, Back up seed
    phrase (not on a view-only wallet), Show wallet code. The board draws
    Your profile in Display; it stays in Wallet, beside what else is this
    wallet's. Nothing was a stopgap to remove: Contacts and Your profile
    were each one row already. There's no fingerprint row: Fob always asks
    for the phone's lock before money leaves (decision 13), so there's
    nothing to set.
147. **Display**: Theme and Currency are rows with their value at the end
    and a chevron ("Same as the phone", "US dollar", the code when a
    currency has no name), each opening its choice in a dialog; the three
    theme choices no longer sit in the card. "Show amounts in" keeps its
    segmented control (decision 3 of the plan), 48 dp tall for touch, with
    the board's "Tap the balance to swap any time" (Home's balance does)
    and the group named for TalkBack. Currency and Show amounts in show
    only with prices, as before. The board's "Address check" row isn't
    drawn: Fob has no setting for it.

## Round 3: the emulator pass

148. **Rotating keeps where you are; Fob isn't portrait only**: nothing in
    the decisions asks for portrait, and Send and the scanners have
    landscape layouts (decision 92's 62% sheet), so the screen still turns.
    The activity takes rotation, screen size, keyboard, density and dark
    mode changes itself (`configChanges`): Compose redraws in place, so
    every screen, its sheets and what's typed in it stay. Anything else
    that makes the activity again (font size, language) keeps the back
    stack in a ViewModel (`NavHolder`), and a `monero:` link that opened
    Fob isn't read a second time then. After Android ends the process, Fob
    opens at Home, as before.
149. **A busy photo still fits**: a picked photo full of fine detail
    didn't fit 1.5 KB at 64 pixels and quality 20 with Android's encoder
    (found on the emulator, where the same picture fits easily with
    desktop libwebp), so the ladder of decision 109 goes on to quality 10
    and 0 and a 48-pixel side before refusing. Each step is a 96-pixel or
    smaller encode, so the extra ones cost nothing noticeable.

## Round 3: review fixes

150. **A photo's pixels are checked before anything draws it**: Rust reads
    a WebP's headers (a lossy or lossless image, alone or after an
    extended header with its alpha) and refuses one claiming more than
    128 pixels a side, an animation, metadata, a chunk it doesn't know, or
    sizes that don't add up; a few bytes can otherwise claim a picture
    that takes a gigabyte to decode. A photo kept by an older build that
    fails this is dropped when the wallet opens (the contact or profile
    stays). Android's Avatar reads the bounds again before decoding,
    decodes off the main thread scaled to its size on screen, caches what
    it decoded, and shows the initial for anything it can't.
151. **Only the share just made swaps by touching phones**: Rust keeps the
    share the last Share my profile made, and starts a swap only on it,
    only while it's still "Code shown" and until its swap ends (the
    screen closing ends it). A share opened again from Your shares or
    Activity, an older share, or one whose link was copied or shared is
    refused, and its screen shows the code without the touch strip; Copy
    link or Share ends a swap already running on it. So a second person
    never gets an address someone already has. A screen made again by a
    font or language change ends its swap too, and shows the code alone
    after.
152. **The card holds back its last chunk**: it answers with its own next
    chunk, not the one at the command's index, and gives nothing in place
    of its last until the reader's frame is whole, matches its checksum
    and reads as a profile for this network (a frame that doesn't is
    refused at once, "not a profile" told apart from garbled). A reader
    that isn't a Fob gets at most all but the last chunk, and nothing is
    recorded as gone out. The reader takes an empty answer as "waiting"
    while some of its own frame is still to go. Both sides of a phone are
    the same build, so the frame's version stays 1.
153. **The card side doesn't claim the other phone got ours**: the reader
    knows (the card said so); the card can't, since its last answer may
    not have arrived. Swapped on the card's phone says "You got Sam's
    profile and sent yours. If Sam's phone doesn't say Swapped, touch
    again." A further acknowledgement round would only move the same
    doubt to the other phone.
154. **One swap, one other phone**: once the reader side is finishing with
    a profile, the card answers "not available"; once the card side has
    one, an exchange the reader was in takes nothing. Ending a swap reads
    what came and stops it under one lock.
155. **An address paid as a named code's stays one they were paid at**:
    Paid's offer and "Paid, not saved" both take the name from the newest
    payment to the address that came from a code with a name, so paying
    it again without the name (from Paid before, or pasted) never offers
    it as a send address.
156. **Rust decides what a payment code carries over**: Send hands Rust the
    code its form was filled from, as read, with the address, amount and
    note as they stand. The code's name goes with the payment while it
    goes to the code's address (an amount edited, or its unit switched,
    keeps it; decision 111); its receipt only while the amount is the
    one the code asked too, so a switched or edited amount can't arrive
    with a merchant's receipt for another. A code without an amount
    carries no receipt.
157. **Nothing is sent or shared from an amount Rust is behind on**: the
    amount field's word from Rust says what text and unit it was made
    for, and Review, Copy and Share wait until that's what the field
    holds. The chip fetches the converted text and Rust's word on it in
    one go and sets them together.
158. **A code seen while To is filled is asked about**: with the form
    pulled up and To filled (typed, pasted, picked, or an earlier code),
    a code the camera sees to another address shows "The camera found
    another code, to 84Bv … 9fQ2. Use it instead?" with Keep and Use it;
    anything else it sees is left alone. Editing To doesn't take the code
    still in view again; clearing it does. A code seen while the fee is
    being fetched isn't marked as read, so it's taken after. A link's
    code is read once even if the screen is made again.
159. **The note follows the code**: taking a code sets the note to its
    description, or clears it; the form kept under Confirm keeps the note
    Confirm showed.
160. **A line about a payment says what was paid**: Paid before, a
    contact's lines and card, and "Paid, not saved" show a payment's worth
    when it was sent, else its XMR, never a receipt's total (what the
    payee wrote). The shop's name from the receipt may still title it.
161. **Send's small fixes**: the sheet's handle is 48 dp to touch, its bar
    still small; in a landscape strip too short for words over the camera,
    what a scan found shows on the folded sheet instead.
162. **Names and notes keep nothing that draws nothing, and lookalike names
    are one name**: cleaning drops Unicode's default ignorables (zero-width
    spaces and joiners, bidi marks including the Arabic letter mark, line
    and paragraph separators, invisible operators, fillers, soft hyphens,
    tags), keeping a joiner between two emoji and a variation selector or
    tag after one, which make one emoji of several. Contacts are matched
    by name on a skeleton: cleaned, NFKC (`unicode-normalization` 0.1.25,
    a new dependency, small, its `tinyvec` already in the tree, released
    well over two weeks ago),
    lower case, without spaces, Cyrillic and Greek letters drawn like
    Latin ones mapped to them (and 0, 1 and | to o, l, l). A scanned
    profile named "Ѕam" with a Cyrillic Ѕ, or "Sa​m" with a zero-width
    space, asks first, saying "You have a contact whose name looks like
    this one: Sam." The map is small and hand-made (Unicode's full
    confusables table would be a new dependency): it catches the
    lookalikes of Latin names, not every one there is.
163. **Rust checks a scanned profile before adding it**: Add contact on a
    profile whose address is already a contact's is refused ("That
    address is Sam's already: update Sam instead."), and the ask no longer
    offers "Add as new contact" for it: two contacts never own one send
    address from a profile. A profile whose name is (or looks like) a
    contact's is added only once asked (Add as new contact), whatever the
    screen captured when it scanned.
164. **A send address replaced keeps its payments with the contact**:
    changing or clearing a contact's send address (Change on the card,
    Edit, or Update from a profile) moves the old one into "Also paid at"
    when anything was paid to it, so its payments stay on the card and out
    of "Paid, not saved". One never paid is just replaced.
165. **A code's description is cleaned like a note**: control characters,
    bidi marks and the rest of decision 162's invisibles out, trimmed, cut
    to 120 characters, so a payment's note (shown on contacts' cards) is
    never turned round on screen. A typed note going with a payment is
    cleaned the same way. Change on a card that can't be saved says why
    and stays on the scanner's sheet.
166. **A name in a code is cut to 120 bytes as well as 40 characters**: 40
    letters of most scripts, 30 emoji. With the note's 360 encoded bytes,
    the most a request's code holds is 886 bytes: past version 20 at
    medium, so it's made at low, version 21 (tested). A usual request
    (a name, a short note) stays at medium, version 10 or below. The
    largest profile code drops from 2664 bytes to 2544.
167. **The same ask again gives the same request**: Share or Copy pressed
    for what the newest request already asks (the same contact, amount as
    typed and its unit, note, and name in the code), while nothing has
    come to it, gives that request again rather than a new address; the
    rule is Rust's now, and Receive also ignores a second tap while one is
    being made. Any change, or a payment to it, makes the next one new.
    Only the newest is reused: an older alike request stays as it went.
168. **A deleted request names exactly what was in when it went**: delete
    keeps the transactions already paid to its address (the pool's too),
    and only those still say who it was from; a payment seen in the pool
    and mined later, in a block whose time is after the delete, stays
    theirs. A request deleted before this was kept goes by time as before.
169. **One payment to several of this wallet's addresses credits each its
    own part**: Activity keeps what came to each address of a payment in
    (and the pool's, kept per address as the pool scan finds them), and a
    request or share counts its part only. The payment's own line is
    still one, titled by its first address.
170. **A request's code again is the one that went out**: the name put in
    it is kept on the request, so renaming yourself or turning the switch
    off later doesn't change an old request's code.
171. **Restores look further and hand out past what another phone may
    have used**: scanning watches 200 subaddresses past the highest one
    handed out or paid (was 50; wallet2's gap limit), and a payment found
    near the edge moves the edge on from the next block, not the next
    batch. A wallet restored from words or a code hands out its first new
    address 50 past the highest one found paid (51 when none was), and
    again past any older one found later, so the other phone's requests
    and shares that were never paid aren't given out a second time and a
    late payment to one isn't taken for a new request. What's left: a
    payment more than 200 addresses past the last one found isn't found
    by a restore, and an old phone's unpaid addresses more than 50 past
    its last paid one can still be handed out again; neither phone's file
    says how far the other went. A wallet restored before this build
    reads as one made here and skips nothing.
172. **A payment once sending is never left unrecorded**: Rust publishes a
    signed payment and records it (inputs spent, the payment and what only
    the sender knows) in one task of its own, so dropping the call (the
    app leaving Confirm, which cancels the coroutine and frees the Rust
    future) can't stop it between the node taking it and the wallet file
    keeping it. Android sends on the model's scope, and Back does nothing
    on Confirm while it says "Sending…".
173. **A prepared payment left behind is forgotten**: Confirm forgets its
    prepared payment whenever it's left without sending (system Back, the
    gesture, the top bar), not only by its top bar, unless the screen is
    only being made again; and Rust forgets any prepared payment after 15
    minutes, so one left behind (a recreated Send's, say) never piles up.
    Sending one that's gone says it was already sent or has expired.
174. **The phone-lock prompt survives the activity being made again**: the
    prompt is made in `onCreate` with its answer kept in the activity's
    ViewModel; a Confirm that comes back while it's up waits for that
    answer and sends on it, rather than losing it to a dead coroutine.
175. **Coins and Home read well with TalkBack**: Coins' bar reads Rust's
    "1.8592 XMR ready, 0.6221 XMR not yet, 37% of the way", not a bare
    percentage; Home's bar stays silent (its card says the same). Home's
    opened card is one line per sum, each read as one ("Ready to spend,
    1.8592 XMR, $308.70"), the money right-aligned in a column 80 dp wide
    so the amounts still line up. Home's sums and its not-ready line come
    from one look at the wallet, so they always add up. The Coins board
    keeps the two-part bar: its template draws Home's bar, and the per-coin
    list it computes (`bar`) is never drawn.
176. **A new wallet reminds until its words are written down**: the wallet
    file keeps that a wallet made here hasn't had its seed phrase said to
    be written down, so if Android ends Fob during the backup step (the
    user off writing them down), Home shows Rust's "Back up your seed
    phrase · Your 16 words aren't written down yet. Without them, this
    wallet can't be restored." on the highlight colour above the rest,
    opening the words with "I've written them down"; pressing it there or
    in the first backup step ends the reminder. Restored wallets, and
    files from before, never show it.
177. **The photo ladder steps down as decision 109 had it first**: quality
    80 to 20 at 96, 72 and 64 pixels; only when none fits, quality 10 and
    0 at 64, then at 48, so a middling photo isn't taken blocky at 96 when
    72 at quality 80 would do, and a busy one still fits (decision 149).
178. **Settings' currency words come from Rust**: the Currency row's value
    ("US dollar", or the code for one Fob has no name for) and each choice
    ("US dollar · USD", or the code).
179. **45 currencies, each written one way**: the table in
    `price/currencies.rs` gives each code its English name, a plural for
    labels ("Amount in Japanese yen"), a symbol and its decimals. Symbols
    are kept unique among the 45: "$" is the US dollar only; A$, C$, NZ$,
    HK$, S$, NT$, R$, MX$, CN¥ (¥ is the yen), R for the rand, Rp, RM and
    the single-sign ones (£ € ₹ ₩ ₫ ₪ ₺ ₴ ₦ ₽ ₱ ₾ ৳ ฿). Where no short sign is
    clear (kr, zł, Kč, Ft, the dinars, riyals, dirham, rupees of Pakistan
    and Sri Lanka, CHF, the pesos of Argentina and Chile, the Bermudian
    dollar, the kyat) the code goes after the amount ("412.06 CHF").
    Decimals are ISO 4217's (3 for BHD and KWD, 0 for JPY, KRW, VND and
    CLP) except the rupiah and the forint, written in whole units (0)
    everywhere. Settings' Currency choices ("US dollar · USD", decision
    178) give way to the list of decision 186.
180. **Kraken prices 7, not 8**: its AssetPairs lists USD/AED, but that
    market has never traded (the ticker's last price is 0, OHLC has one
    empty candle), so Kraken can't price the dirham and isn't offered for
    it; CoinGecko is. The others: USD and EUR from XMRUSD and XMREUR; GBP
    and AUD as XMR/USD divided by GBP/USD and AUD/USD; CAD, CHF and JPY as
    XMR/USD times USD/CAD, USD/CHF and USD/JPY. Each rate is the market's
    last trade, both asked at once. History asks both daily OHLCs and joins
    them by day; a day with no exchange candle takes the last rate before
    it. On 2026-10-31 Kraken's way gave A$844.63, CoinGecko A$844.02.
181. **CoinGecko, keyless**: `simple/price` for now and the daily
    `market_chart` for 365 days (the keyless limit; more is refused), each
    day's first point (its start, 00:00 UTC) as that day's price. HTTP
    429, or a body saying error 429, is "Busy, try again soon"; so is
    Kraken's "Rate limit exceeded". Prices come as JSON numbers, read from
    their digits into millionths (from the double only when written with an
    exponent).
182. **Old wallet files**: prices off becomes XMR only; on keeps its
    currency (USD or EUR, both Kraken's). A file from before prices is in
    dollars, as is a new wallet. The settings no longer write `on`; a
    `null` currency is XMR only. A code Fob doesn't have becomes XMR only,
    and a provider or transport that doesn't fit is moved on opening, as
    choosing would. Records of past prices stay keyed by currency, so a
    payment priced in dollars shows its dollars again when dollars are
    chosen again.
183. **Moving to a provider that fits**: Fob keeps the chosen provider
    while it prices the currency (back to dollars from New Zealand
    dollars, CoinGecko stays); otherwise it moves to the first that does,
    Kraken before CoinGecko, saying "Kraken doesn't price New Zealand
    dollars, so Fob now asks CoinGecko". A provider that can't price the
    currency is refused. Picking one without Tor while on Tor moves to the
    web: "Kraken doesn't answer over Tor, so Fob now asks over the web".
184. **Web and Tor per provider, until Tor exists**: Kraken web only,
    CoinGecko both, as placeholders to confirm when Tor is built (said in
    each provider's `answers_over`). Tor itself stays refused as before.
    Failures carry a kind (not answering, busy, no price in the answer,
    Tor not built), so Settings' Price from row reads "Kraken · over the
    internet · Not answering" rather than a library's message.
185. **Price from's check**: every provider that prices the chosen
    currency is asked for the price now, all at once, 8 s each at most,
    over the chosen transport; one that doesn't answer over it is never
    asked (over the web while Tor is chosen would show the phone's
    address) and stays pickable. Each line: rate ("A$845.10") or the
    failure in words ("Not answering", "Busy, try again soon"), whether it
    can be picked, and a TalkBack label ("Kraken, web only, A$845.10").
    Answers aren't kept as the current price: the check is only a look.
186. **The "Also show amounts in" list**: groups NONE (XMR only, "Fob never
    looks up a price"), THIS PHONE’S (the locale's currency, when it's one
    of the 45) and ALL 45 by name, each row with its sign (symbol or
    code), name and "AUD · Kraken, CoinGecko". Searching matches name,
    plural, code or symbol, case and accents aside ("zloty" finds the
    złoty); it drops the phone's row (it's among the rest), titles the
    rest "3 OF 45", finds XMR only by "xmr", "monero" or "ɱ", and says "No
    currency matches “…”" when nothing does.
187. **Android until step 3**: Settings keeps a stopgap: "Also show amounts
    in" opens the list in a dialog (Rust's groups and rows), "Show first"
    only with a currency, "Price from" opens a dialog of the check's lines
    and the two transports; a move's words show as a toast. The Show
    prices switch and `set_prices_on` are gone.
188. **One run of failures, held in Rust**: every price lookup says who
    asked: the 5-minute refresh (background), or the user (pull, Retry, a
    changed currency or provider, a money amount typed). A failure the
    user caused always shows the toast; the background's shows only when
    it starts a run, and later background failures in the run update the
    words without showing again. A lookup that works ends the run and
    takes the toast down; a lookup with nothing due (asked a minute ago)
    leaves the run as it is. The app shows a toast whose id is new: a new
    id is a new showing, so a toast swiped away stays away until the user
    does something or a later run starts. Changing currency or provider
    forgets the run (it was about settings gone) and asks at once, as the
    user. The run isn't kept in the wallet file: a new start begins clean.
189. **The toast's words**: title "Couldn't get the USD price", or "Couldn't
    get 29 Oct's USD price" when only a day's history failed (the newest
    day a payment lacks a price for). Line: who failed and how ("Kraken
    didn't answer", "CoinGecko is busy", "Kraken answered without a
    price", "Tor isn't built yet") and what Fob uses instead: "using the
    14:02 price" (or "yesterday's 14:02 price"), "shown as not available"
    for history, "amounts stay in XMR" for a price over a day old. With no
    price ever kept for the settings the line is the boards' "No price yet
    · amounts stay in XMR", or "No price yet · type the amount in XMR" when
    the lookup came from a money amount typed (ToastStates and ToastSend);
    the Settings row still names the reason. Retry: "Trying again…",
    "Asking Kraken for the USD price", then "USD price updated", "1 XMR =
    $166.10". Straight apostrophes, as Rust's other words.
190. **A price is shown for a day**: amounts keep the last current price
    for up to 24 hours (it was an hour), and money can be typed on Send
    and Receive against it; older, or none, and amounts are XMR only and
    the amount field says "No USD price now · type the amount in XMR".
    A payment is still only priced at its moment from a price within the
    hour (decision 64's rule stays; decision 63's hour for showing gives
    way). Once a price has missed a refresh (10 minutes old) the amount
    field says when it's from: "≈ 0.1204 XMR
    · price from 14:02" in place of the rate, "1 XMR = $166.10 · price
    from 14:02" while empty. Home's balance and Send's ready line keep
    the price without an age; the toast says when it's old.
191. **Money typed asks again**: an amount field in money with something
    typed, against a price 5 minutes old or more (or none), has the price
    task ask now, as typing, at most once a minute; the task, not the
    keystroke, does the asking, so typing never waits on the network.
192. **Toast placement**: the toast is one composable over the app's root,
    inside the safe area (above the gesture bar and the keyboard), 16 dp
    up, or 8 dp above the screen's bottom bar: `Page` reports its bottom
    slot's height, Send's open sheet the Review column's, through
    `LocalBottomSpace`; a screen going takes its height with it. Inverse
    roles were added to the theme (`inverse`, `onInverse`, `inverseMuted`,
    `inverseAccent`, `inverseGreen`): dark card on the light theme, light
    card on the dark one. Swiped down (half of 96 dp) or sideways (96 dp)
    it goes; shorter swipes spring back. TalkBack reads it as a polite
    live region and offers Dismiss as an action, the swipe's stand-in.
193. **Price from is a screen now**: the cog and Settings' row open
    `Screen.PriceFrom` (step 1's stopgap lines and transports, unchanged),
    so Back returns to the screen the cog was tapped on. Picking a
    provider or transport goes back, as the dialog closed. Settings' Price
    from row is red with Rust's line while lookups fail ("Kraken · over
    the internet · Not answering").
194. **Settings as board SettingsV3**: the Wallet card has your profile,
    Contacts and Backup and keys, each with its icon in a sunken circle;
    Display has only Theme; Money (decision 195) replaces Display's
    currency rows and the Prices card; Syncing, Network, Receipts and
    This phone stay. Backup and keys opens a stopgap screen until step 7:
    Seed phrase (not on a watch-only wallet) and Wallet code under "Back
    up", each opening its screen as before. Its line says only what's
    there ("Seed phrase, wallet code", or "Wallet code" watch-only), not
    keys, which step 7 adds.
195. **The Money card's words**: "Also show amounts in" says "Uses today's
    XMR price", or with XMR only "Fob never looks up a price" (Rust's
    `shown_in_line`), its value at the end (a long name wraps to two
    lines). Price from, while lookups fail, shows the toast's own line in
    red ("Kraken didn't answer · using the 14:02 price"), as board
    PricesWording has it, with Try again in place of the chevron; this
    replaces decision 193's "Kraken · over the internet · Not answering".
    Try again works with the toast swiped away too: it asks Rust for the
    current failure and shows Retry's progress in the toast.
196. **The currency list is a screen**: `Screen.CurrencyList`, with
    Contacts' search pill (now one shared `SearchField`, here 52 dp, with
    a clear button and the keyboard's Search closing the keyboard). Rust
    filters on every change; groups are headings for TalkBack, rows are
    radio buttons reading name and providers (the sign in the circle
    isn't read; three-character signs drop to 13 sp). Choosing goes back.
197. **Move words are an in-app notice, not a system toast**: Rust's
    "Kraken doesn't price New Zealand dollars, so Fob now asks CoinGecko"
    shows for 4 s in a small card in the price toast's colours, just above
    where the toast sits (above it when both show), read politely by
    TalkBack; tapping it dismisses it. The model posts it from
    `setCurrency` and `setPriceProvider`, so any screen choosing shows it.
198. **Price from stays put**: picking a provider or a transport keeps the
    screen open (decision 193 went back); the radio follows the settings.
    The check runs as the page opens and again when the currency or the
    transport changes, not on a provider picked (its rate is already
    there). While it runs, one row says "Asking each provider…" with a
    spinner. The intro is the board's: "Prices in **Australian dollars**,
    checked as this page opens. Tap a provider to pick it." Each provider
    is one line: radio, name, globe and onion badges for what it answers
    over, rate on the right or the failure in red, the name greyed and
    not pickable when it failed; TalkBack reads Rust's label only.
199. **"Ask over" words from Rust** (`transport_rows`): Web, "The provider
    sees this phone's IP address"; Tor, greyed with "Kraken doesn't answer
    over Tor" when the provider lacks it, else "Hides this phone's IP
    address · not built into Fob yet" while Tor isn't built (greyed too),
    and the board's "Hides this phone's IP address; slower" once it is.
    The ffi's own words for these (the "Not available yet" note and
    "Kraken · over Tor") are gone.
200. **The profile row** (decision 11): Rust's `settings_row` gives a
    title and, with a name, the name and " · share as a code" apart, so
    the app cuts only the name with an ellipsis and the rest always shows.
    No name (and a blank one from an old file, photo or not) is only
    "Add a shareable Fob contact" with the person outline; the photo shows
    only beside a name.
201. **Amounts say how old their price is** (decision 9, over decision
    190's "without an age"): once the price is 10 minutes old, Home's
    money figure and Send's ready line add "· 40 min old" or "· 2 h old"
    (whole minutes under an hour, whole hours after; never days, as a
    price over a day isn't shown). `Balance` gains `fiat_age` and
    `fiat_line` ("$412.06 · 2 h old"); with money first, the big figure
    stays bare and the age sits small beside it. `balance` and
    `ready_line` take the time, as the toast does.
202. **No price yet still says why**: the toast's line with no price ever
    kept is "CoinGecko didn't answer · no NZD price yet" (board
    ToastSettings), or "… · type the amount in XMR" when a money amount
    asked; decision 189's "No price yet · …" lines are gone.
203. **Split up's notes in every currency**: each of the 45 has its
    smallest note in the table (`note`), a round amount on the 1-2-5
    ladder worth about five US dollars at late-2026 rates ($5, €5, £5,
    A$5, NZ$10, ¥1,000, ₩5,000, ₫100,000, Rp50,000, 1 KWD, 2 BHD, 5,000
    ARS…), and the six notes run up the ladder from it (¥1,000 to
    ¥50,000). Notes are written with thousands separators, and a tile's
    label shrinks to fit ("₫5,000,000"). The table is fixed, not from the
    live rate, so notes stay the same round amounts as XMR moves; a
    currency that loses much of its value (the Argentine peso) needs its
    entry raised in a later build.
204. **Link speed** (decision 2): the transport counts the bytes of every
    `get_blocks.bin` response the block fetcher reads (not decoys or JSON
    calls, and no extra requests). The sync loop samples that count and
    the scanned height at each report; blocks per second and MB/s are
    measured from the newest sample back to the last one at least 5 s
    older, and say nothing until 3 s have been measured. A catch-up's
    measurements start again after it ends, after a failure and after a
    wait. MB are decimal: "1.8 MB/s", "12 MB/s" from 10 up, "640 kB/s"
    under one; blocks "28/s", "0.8/s" under ten.
205. **Time left** (decision 1): none until the speed has settled, and the
    first one shown must be at least 10 s, so a catch-up that ends within
    about 13 s shows only Starting, then Synced. After that it never goes
    up (plan decision 1, restored after review R4: this entry first let
    it rise when a node slowed a lot). The estimate is taken over the
    last 20 s (the shown speeds over 5 s), so the wobble of block sizes
    along the chain doesn't move it; the shown time counts down with the
    clock, takes a lower estimate at once, and holds through a higher
    one. When the estimate rises past it by half as much again and 30 s
    more, or blocks stop for the whole window, the number is dropped
    (chip "Catching up", card "Working it out"), and a fresh countdown starts
    once 10 s more has been measured; a smaller number is never replaced
    by a larger one. Rounded for reading: under a minute
    up to the next 10 s ("20 s"), then the nearest minute ("6 min"), the
    nearest 5 minutes under 10 hours ("1 h 10 min"), then whole hours.
206. **The chip's words** come from Rust: "Starting", "6 min left",
    "Synced", "Can't reach node", and for TalkBack "Catching up, 34%,
    about 6 minutes left" (minutes and hours spelt out). The percent rounds
    down and stays at 99% until synced. A failed round shows the red dot
    even when the wallet was synced, since the node didn't answer the last
    poll (before, a synced wallet hid the failure).
207. **The details card** opens in every state. Catching up: the board's
    rows, with "Working it out" and "Measuring" before the speed settles,
    and "You can still receive while Fob catches up." for a watch-only
    wallet. Synced: "Synced", Payments up to and Latest block, no bar and
    no "checked … ago" (kept simple; Node's checks come in step 8). Can't
    reach node: the rows that still hold, "Fob keeps trying on its own."
    and the error in red, moved here from under Home's balance with the
    bar and sync line. Payments up to reads "Today, 14:20",
    "Yesterday, 14:20", "29 Oct, 14:20". Node settings closes the card and
    opens Node.
208. **The card is a popup** under the chip against the screen's end edge,
    at most 300 dp wide; tapping outside it or Back closes it, and the chip
    is a button with expand and collapse actions. The ffi's `SyncStatus`
    drops `seconds_left` and `up_to` (only Home's sync line read them) for
    `chip_look`, `chip_text` and `chip_label`; `sync_details` gives the
    card for the phone's clock.
209. **Send's sheet fits its form** (decision 3): pulled up it is the
    form's own height, Review straight under the ready line, up to the
    screen under the top bar; only past that (a small phone, big font,
    the keyboard up) does the form scroll, Review kept below it. The
    keyboard's height counts as part of the sheet, so Review stays above
    it. The small frame is up to 200 dp, centred in the camera's room
    above the sheet, and hidden when that room leaves it under 64 dp.
210. **The sheet follows the finger** with a plain height state of
    its own (not `AnchoredDraggable`, whose anchors would have to be
    rebuilt each time a stop's measured height changes): dragging the
    handle sets the height, clamped between the two stops; let go, a
    fling over 125 dp/s (Material's sheet threshold) picks its way,
    otherwise the nearer stop, and a 300 ms ease on the board's curve
    takes it there. Every other open or fold (tapping the handle, Back,
    a scan, a contact, a failed Review) makes the same move. Dragged above
    its folded height the sheet already shows the form (clipped to the
    height), so the form is measured before the finger reaches the top;
    the camera's frame and words follow the live height.
211. **At rest the sheet follows its content at once**: a line appearing
    (an error, "Use this code instead?") or the keyboard rising resizes it
    without the old 280 ms content animation, which would trail the
    keyboard. The handle alone is the drag area; the form and the folded
    list scroll as before.
212. **Your photo in Home's top bar** (decision 4, board HomePeople C):
    a 48 dp button between the sync chip and Settings with the avatar at
    32 dp, or a person outline in a sunken circle while there's no
    profile name (a photo alone shows the outline, as Settings' row
    does). TalkBack reads Rust's title for Settings' row, "Your profile"
    or "Add a shareable Fob contact", so no new words. It opens Your
    profile, which goes on to setting one up while there's none, the same
    way as Settings' row.
213. **The model keeps your profile** (`WalletModel.you`: the profile and
    its row) read when a wallet opens and again when `setProfile` saves,
    so Home has it at once on coming back from editing it. Home is made
    again on each return anyway, but loading there would flash the person
    outline before every photo.
214. **The top bar gives way in a fixed order.** Its end (chip, photo,
    Settings) always fits whole. When the row is too narrow, the word
    "Fob" hides first, then the chip's words (its ring or dot stays, and
    TalkBack still reads its label), then the Stagenet badge. On a 360 dp
    phone at normal font with the badge and "6 min left", the word goes
    and the rest fits. At 200% font the chip shows only its ring and the
    badge stays. A custom layout measures this rather than a guess from
    text sizes. The mark always shows. The chip's target is at least
    48 dp wide when it has no words.
215. **Activity's header wraps instead of squeezing**: the title, then
    Contacts (board order: before Coins), then Coins, in a flow row. When
    a narrow phone or a big font leaves no room beside the title, the
    chips go to a line under it, one under the other if need be. The
    "Still finding older ones" note always gets its own line under the
    header now. Beside the title it never fitted next to two chips on a
    360 dp phone. The title is now marked as a heading for TalkBack.
216. **The fingerprint is asked at Backup and keys' door** (decision 5):
    Settings' row asks for the phone's lock (fingerprint, face or screen
    lock, the same prompt as Confirm) and opens Backup and keys only on
    success; cancelling leaves you on Settings. Seed phrase, Wallet code
    and Keys are reached only from the hub, so the visit is unlocked by
    being past that door on the stack: hub to Keys and back, then to Seed
    phrase, asks nothing more. Leaving to Settings and coming back asks
    again. A second tap while the prompt is up is ignored, and a prompt
    up while the activity is made again (font size, language) still
    answers Settings, as Confirm's does. A phone with no lock set opens
    it straight away, as sending does. Home's backup reminder still opens
    the seed phrase directly: it is a new wallet's backup step, as before,
    not Backup and keys.
217. **Seed phrase's line says the count and the backup**: "16 words
    that restore everything" (25 for a classic seed), then "· backed up"
    for a wallet made here whose words were said to be written down, or
    "· not written down yet" while Home's reminder is still up. A
    restored wallet says nothing more: its words came from somewhere
    already, and one restored from a spend key never had them shown. A
    file from before the reminder counts as backed up, as Home treats it.
218. **A view-only wallet's Backup and keys** has Wallet code and Keys
    only; its warning reads "Anyone who sees these can watch your money.
    …", its footer "This wallet only watches. The code and keys are for
    moving it to another app.", Settings' line "Wallet code, keys". Its
    Keys page has the secret view key alone under Secret, and all four
    public values: the file keeps the address's public spend key; only
    the secret spend key and seed are missing, which it never stored.
219. **The restore height is dated only when the date can be named**:
    within a year of the network's anchor block, where the two-minute
    estimate is days out at most. Further back, blocks didn't average two
    minutes (mainnet's first two-minute block, September 2015, estimates
    as March 2016), so an old height shows as the number alone. It is the
    wallet code's height (the first block scanned, or where scanning
    starts for a new wallet before its first sync). Copy puts the number
    alone on the clipboard, as other wallets take it.
220. **Every Copy on Keys marks the clip sensitive**, the public keys and
    address too (EXTRA_IS_SENSITIVE on Android 13+, the same extra by name
    before, which some keyboards read): the address ties payments to you,
    and one rule is simpler to check. The button's icon turns to a tick
    and "Copied" shows under the value for 2 s, read by TalkBack;
    Android 13+'s own clipboard notice shows too, with the content hidden.
221. **Keys' values wrap at any character**: the 64-digit keys and the
    95-character address have no spaces, so Compose breaks them where the
    line ends; monospace at 12.5 sp, as the board. The hub and Keys block
    screenshots, as Seed phrase and Wallet code already did.
222. **A node's checks and what stops what** (decision 10): Certificate
    (https only), Answers, Right network, Up to date, Sends blocks to Fob,
    in that order. A certificate that isn't accepted, or no answer, skips
    everything after it (skipped rows show no result; TalkBack reads "not
    checked"); the wrong network skips Up to date and Sends blocks. Up to
    date failing doesn't stop Sends blocks: a node that's only behind can
    still be used "anyway". When the node doesn't answer at all, the
    Certificate row is skipped too, not failed: it was never seen.
223. **Up to date's measure**: behind is the node's own target height, or
    the tip this wallet last saw if higher, less its height; more than 5
    blocks (about ten minutes) fails. A last block more than an hour old
    fails too (a stuck node that thinks it's synced). The words are the
    larger of blocks behind × 2 min and the last block's age: "20 min
    behind", "5 h behind", "2 days behind". Passing reads "block 1,792,004
    · 1 min ago" ("just now" under a minute). The last block comes from
    `get_last_block_header`.
224. **Timings**: Answers allows 5 s ("Didn't answer in 5 s", as the
    board), the last block header 5 s, and the block sample 10 s ("no
    blocks in 10 s"). The sample is the hundred blocks before the two
    newest, through the same pipelined fetcher that syncs; blocks/s with
    one decimal under 10, MB/s from the bytes it read (the sync card's
    link-speed words).
225. **Verdicts**: "Working", "Checking…" while any check runs, else the
    first failed check's: "Certificate not accepted", "Doesn't answer",
    "Wrong network", "Not up to date", "Doesn't send blocks". The list's
    lines: "120 ms · in sync" (" · fastest" on the quickest at sending
    blocks when two or more pass), the failed Answers row capitalised
    ("Didn't answer in 5 s", "Refused to connect", "Address not found",
    "Not a Monero node", "Wants a login" for a 401), "Mainnet node: this
    wallet is stagenet", "Certificate self-signed, not accepted", and
    "<ms> ms · 2 days behind" or "<ms> ms · no blocks in 10 s". Use shows
    only on nodes that pass every check; a lagging one in the list has no
    Use (add it in the sheet to "Use it anyway").
226. **Self-signed certificates are checked by Fob, not reqwest**: every
    node connection now uses a rustls config of its own with the usual
    verifier (Mozilla's roots, as reqwest's default) wrapped to note what
    it found. That's how the Certificate row can say "self-signed, not
    accepted", "expired", "not valid yet" or "for another address" rather
    than a TLS error string. With the node's "Accept self-signed
    certificates" mark, any certificate passes (the handshake's signature
    is still checked against it, so the link is encrypted to whoever holds
    that key; nothing says who that is) and the row says "self-signed,
    accepted"; a certificate that verifies anyway reads "valid". An unknown
    issuer (a private authority) counts as self-signed: the box accepts
    both. Price providers keep their own reqwest clients with reqwest's
    usual verification, untouched.
227. **The mark is kept with the node** in the wallet file
    (`node_self_signed`, off for older files) and set only by switching:
    `set_node(url, self_signed)`. Changing node drops the connection, so
    the next request uses the new setting. Default nodes never have it.
228. **Use in the list switches at once**, with no confirmation: the node
    just passed every check, and switching back is one tap. The node left
    stays on the list for this visit (added to the checked ones), and
    every check runs again for the new node. A node checked in the sheet
    and closed without Use joins the list for this visit; nothing about
    added nodes is saved beyond the one in use.
229. **The refresh ignores taps while checking** (it keeps the label
    "Check again" with the state "Checking" for TalkBack) rather than the
    board prototype's "Stop checking": leaving the screen stops checks.
    Opening the screen and refresh check the node in use and the list at
    once.
230. **Settings' Node line** is Rust's "<verdict> · <host:port>": the last
    full check of the node in use this run, else "Can't reach node" while
    syncing fails, "Working" once it has synced or seen a tip, else the
    address alone. Fob doesn't check the node on its own on every sync
    yet (the board's note); the line reads syncing instead.
231. **The sheet's buttons**: "Check" alone until the first check; then
    "Check again" and "Use" (or "Use it anyway"; greyed when neither
    applies). Editing the address or the box clears the checks back to
    "Check". The board's one-line hint under the field stays; there's no
    explanatory paragraph.
232. **Addresses show host and port only**: no scheme, and never a login
    from `user:pass@` in the URL.

## Round 7: the emulator pass

233. **Price from's row names the Ask over row**: Settings reads "Kraken
    · web" or "CoinGecko · Tor", the provider and the name of the row
    picked under Ask over (lower-case "web" after the dot, "Tor" as a
    name), from Rust (`Transport::name`, which the Ask over rows use too).
    "over the internet" stays only inside sentences ("doesn't answer over
    Tor").
234. **Why syncing fails, in the checks' words**: the sync card's red line
    (and the catching-up notification) was the transport's debug text
    (`InterfaceError("error sending request: … dns error …")`). Rust now
    says it as the Node checks would: "Address not found" (which is also
    what an offline phone gets), "Refused to connect", "Wants a login",
    "Couldn't connect"; a timeout is "Stopped answering", since syncing's
    limits (15 s to connect, 30 s quiet) aren't the checks' 5 s; blocks
    that don't scan "Sent blocks Fob can't read". Other failures (saving
    the wallet) keep their own words, capitalised.
235. **Settings' Node line follows syncing** (decision 10's board note,
    decision 230): while syncing fails the line is "Can't reach node ·
    host:port", exactly when Home's chip says so, over an earlier check's
    verdict and over having synced before (the old rule hid the failure
    once a wallet had synced). When syncing works again the last check's
    verdict (or "Working") is back. Settings redraws the line as syncing
    starts or stops failing. The Node screen isn't made to re-check by
    itself: it checks the node in use every time it opens (decision 229),
    so its verdict is never older than the visit; Fob still doesn't run the
    full checks on every sync, which would cost the node a hundred blocks
    each time.
236. **Leaving Fob closes Backup and keys**: when the activity stops (Home
    button, another app, the screen off), every Backup and keys screen on
    top of the stack (the hub, Keys, Seed phrase, Wallet code) goes, so
    coming back shows where they were opened from (Settings, or Home for
    the reminder) and opening them asks for the phone's lock again. A
    configuration change (turning, font size) doesn't count, and Recents
    alone (a pause) doesn't either. The phone's own PIN screen stops Fob
    too, but nothing locked is open yet then.
237. **Home's reminder is behind the same lock**: it opens the seed phrase
    only after the phone's lock, the prompt titled "Backup and keys" as
    from Settings. Both go through one `openLocked` in the activity, which
    asks once for a double tap and still opens the screen when the
    activity is made again under the prompt (font size).
238. **monerod's own certificates work once accepted**: monerod's
    self-made TLS certificate is version 1 with no subject, which webpki
    refuses to read at all, so ticking "Accept self-signed certificates"
    still failed with "couldn't connect" (found on
    https://node.monerodevs.org:38089). For a node with the mark, the
    handshake's signature is now checked against the certificate's public
    key read by Fob's own small DER reader (TLS 1.3 through rustls's raw
    key check, TLS 1.2 through the provider's algorithms matched to the
    key's), so the link is still proven to whoever holds that key. Nodes
    without the mark keep webpki's full check.
239. **Photos keep without Android 16's colour profile**: Android 16's
    WebP encoder puts every image under an extended header with an sRGB
    `ICCP` chunk (about 450 bytes), which Fob refused, so every picked or
    taken photo said "couldn't be made small enough". Rust
    (`contact_photo_kept`) now takes the profile out, and the extended
    header when it then says nothing (no alpha), and hands back the bytes
    it keeps; the app's ladder stops at the first it does. Fob's photos
    are sRGB, so nothing changes in how they look. (Decision 149's "busy
    photo that didn't fit" on this emulator was probably this.)
240. **The price toast steps aside for the keyboard**: above the keyboard
    it covered the field being typed in (Receive's amount). While the
    keyboard is up the toast hides and comes back when it goes; the
    field's own line still says how old the price is ("price from 14:02",
    "No USD price now · type the amount in XMR").
241. **The toast's cog keeps forms**: the cog opens Price from over any
    screen, which dropped Send's and Receive's typed forms. Those screens
    now put their form on the back stack as a draft just before (as
    Confirm and New contact already did), so Back finds Send pulled up and
    filled, and Receive's amount and note.
242. **"Trying again…" shows for at least 0.8 s**: offline, Retry failed
    within a frame, so it looked as if nothing happened.
243. **The sync card at a big font**: it scrolls within the room between
    the chip and the gesture bar, rather than being pushed up over the
    chip with Node settings off the screen, and each value keeps its
    longest word whole ("2,240,749", "Today,") while its label wraps.
    Node's checks share that row (`LabelAndValue`), and the node's address
    on the Working card shows whole, wrapping, rather than cut short. On
    Keys a badge goes under its title when both don't fit.
244. **Money's value sits at the row's end**, by the chevron as Theme's;
    a long currency name wraps within about 150 dp.
245. **A marked node's certificate is accepted only for what a node's own
    can have** (review R7): self-signed (an unknown issuer, an authority's
    certificate used as the server's own, monerod's version 1), expired,
    not yet valid, or for another name. A certificate that is broken,
    revoked or wrongly signed by its authority is "not valid, not
    accepted" whatever the mark, rather than read as self-signed; a
    handshake not signed by the certificate's key (someone answering with
    a certificate they don't hold the key of) fails the Certificate row
    as "signature doesn't match, not accepted", not "couldn't connect".
    Fob's node client follows no redirects (monerod never sends one): one
    would take the node's certificate rule, and a name lookup that skips
    the onion check, to another host; it reads "answered 307". The
    connection kept is keyed by the node and its mark together, so a mark
    taken off mid-connection never leaves a relaxed one in use. The
    Working card's mark comes from Rust (`Report::mark`).
246. **Sync failures said truly** (review R4): a failed round measures
    nothing (the pace starts again at the first report after the wait), so
    no speed or time left spans a backoff. The words come from the error's
    kind and the connection's own certificate as the Node checks read
    them: the node answering against the protocol (`InvalidInterface`) is
    "Sent blocks Fob can't read", an older file's onion node "Needs Tor,
    not in Fob yet", a refused certificate "Certificate expired, not
    accepted". A failure saving the wallet isn't the node's: the chip says
    "Can't save", the card "Can't save the wallet", and Settings' Node
    line doesn't blame the node. Switching node clears the old node's
    failure at once (chip and line) and wakes the sync loop. Settings' Node
    line shows a check made after the last failed round over "Can't reach
    node" (a failed round drops the check before it). The card's rows each
    have Rust's spoken words for TalkBack ("28 a second", "1.8 megabytes a
    second", "about 4 minutes"). Sync reports read the link's byte count
    from a transport kept beside the connection, never waiting on a slow
    connect. After the time left is dropped for a slowdown the chip reads
    "Catching up" (the speed is known), not "Starting".
247. **Kraken's prices are its quotes' middle, and the yen is CoinGecko's**
    (review R1): the price now is the middle of the best bid and ask, never
    the last trade, refused ("Answered without a price") when the spread
    is over 0.5% of it on an exchange market or 2% on an XMR one. Kraken's
    USD/JPY trades about a dozen times a day across a 2% spread (its last
    trade on 2026-10-31 was 1.45% off CoinGecko's yen), so Kraken no longer
    lists the yen: 6 currencies, and a file with Kraken and yen opens with
    CoinGecko. GBP, AUD, CAD and CHF sat at 0.006-0.02% spreads with
    thousands of trades a day on 2026-11-01. An exchange market's day
    counts in the history with 100 trades or more, and a rate is carried
    forward 4 days at most (a weekend and a holiday).
248. **Price lookups say what they are** (review R1, R2): opening a wallet
    file never moves its transport (moving off Tor would show the phone's
    address unannounced); a provider kept over a transport it doesn't
    answer over isn't asked at all and the toast says "Kraken doesn't
    answer over Tor" until one is picked. Flipping Show first asks for
    nothing and keeps a run of failures; only a changed currency, provider
    or transport asks. A lookup that comes back after a newer one has
    (Retry over a background lookup hanging on its timeout) changes
    nothing, price or toast. Retry that finds a day's price names the day:
    "29 Oct's USD price found", that day's rate.
249. **Every amount at an old price says how old** (review R3, plan
    decision 9): beside Home's balance and Send's ready line, the confirm
    screen's total ("0.1204 XMR · $20.03 · 23 h old"), Coins' ready sum,
    Home's opened not-ready card ("All ready about 09:02 · in 14 min ·
    price 23 h old"), a payment's Now rate, and Split up's footer ("coins
    are set in XMR at yesterday's 09:00 price · 23 h old"; "today's price"
    only while it's fresh) all carry the age once a refresh is missed.
250. **Receive's code follows the price** (review R2): Price settings
    carry when the shown price was asked (`price_at`), and Receive makes
    its code again when it changes, so the QR, its XMR line, Share and Copy
    all convert at the same price.
251. **No price asks for one** (decision 191's "or none", review R2):
    while a currency is chosen and no price is shown, the amount field
    (then in XMR) has the price task ask as typing, once a minute, when it
    opens or anything is typed, so its failure shows and a price that
    comes back lets money be typed again.
252. **Price from's words are Rust's** (review R3): its intro ("Prices in
    **Australian dollars**, checked as this page opens. Tap a provider to
    pick it.", the plural in bold as a separate part) and the XMR-only
    sentence ("With XMR only, Fob never looks up a price.") come from
    `price_from_intro`; the Kotlin strings and fallback are gone. Once the
    checks are in, a polite live region says Rust's summary ("Kraken
    A$845.10, CoinGecko not answering"), as "Asking each provider…" was
    said when they started.
253. **The toast with the keyboard up, the cog on Confirm, a swipe while
    retrying, money kept** (review R2; decision 240 is replaced): while the
    keyboard is up the toast sits at the top of the screen, under the
    status bar, clear of the keyboard and of the field being typed in
    (which the keyboard keeps just above itself), and is still a polite
    live region, so the failure typing causes is seen and heard while it
    applies. The cog is off (greyed, not clickable) on Confirm, whose
    leaving cancels the prepared payment, sending or not. Swiping the toast
    away while "Trying again…" also swipes the failure that Retry comes
    to. Money typed on Send or Receive when the price goes (Price from
    picked another provider that hasn't answered yet, or a price passing a
    day) is kept aside, never read as XMR, the field empty in XMR; it comes
    back as typed when a price does (and Back from the cog keeps it in the
    draft).
254. **The phone's lock is skipped only when no screen lock is set**
    (review R6): "no lock" is `KeyguardManager.isDeviceSecure` alone, for
    Backup and keys, Home's seed reminder and Confirm alike. The prompt
    takes a strong biometric or the screen lock, except on Android 9 and
    10, where androidx.biometric 1.1.0 takes only a weak biometric with
    the screen lock (their prompt has no strength classes; on 8 to 8.1 and
    11 on the strong pair works). Any other failure to ask on a phone with
    a lock (sensor busy, security update needed) refuses, with a system
    toast: "Fob couldn't ask for your phone's lock just now, so it
    stopped. Try again in a moment." Confirm adds its own "Not sent.".
255. **Leaving Fob cuts the back stack at the first locked screen**, it
    and everything above it (Price from opened over Keys by the price
    toast's cog), so Back never leads to one without the lock.
256. **A copied secret key leaves the clipboard** after 60 s, or when Fob
    leaves the screen, whichever comes first, if the clip is still Fob's
    (a mark in the clip's extras; when Android 10 and later won't let Fob
    read the clipboard after losing focus, a clip-changed listener that
    ran while Fob was in front decides). Only the secret spend and view
    keys: the address, public keys and restore height copy as before and
    stay, since they're for pasting elsewhere. A secret key is still
    marked sensitive. Pasting a secret key into another app therefore
    needs split screen or a paste within Fob's own session (switching
    apps clears it), which is the price of not leaving it behind.
257. **Backup and keys' warning names the phone's lock, not a
    fingerprint**, and only when a screen lock is set (the app tells Rust
    through `backup_hub(phone_lock)`): "Fob hides them from screenshots
    and asks for your phone's lock first." or, with no lock, "Fob hides
    them from screenshots."
258. **Send's sheet settles as Android's animator duration scale says**
    (review R5): its 300 ms is stretched or shortened with the scale, and
    with "Remove animations" (scale 0) it goes to its stop at once, as the
    profile frame on the same screen does. The hand-written frame loop
    stays (rather than `Animatable`) because it eases to a stop measured
    while it runs.
259. **Send's sheet is sized in the layout pass**: a `Layout` measures what
    it holds unbounded and is that tall at rest, or the finger's or the
    settle's height while moving, in the same pass, so it no longer
    flashes a guessed height as Send opens or comes back from Confirm, and
    Review rises with the keyboard in the same frame. The measured heights
    stay in state only for the drag's clamp, the settle's target and the
    camera frame's maths (which may still follow a frame behind).
260. **Money's value goes under its title when both don't fit** (review
    R3): beside the title it takes at most 152 dp and never more than
    leaves the title its longest word; when its own longest word doesn't
    fit in that, or when sitting beside it would wrap the title (and its
    line) onto more lines than they take across the whole row (the
    emulator re-check at 200 % on a 360 dp phone found "Also / show /
    amounts / in" one word a line beside "US dollar"), it moves under the
    title, still at the row's end by the chevron, as Keys' badge moves
    under its title. Show first's XMR/currency switch does the same (it was
    squeezing "Show / first" the same way).
261. **The profile row wraps to two lines at a big font** (review R3): on
    one line the name is cut with an ellipsis so "· share as a code"
    always shows (decision 11), as long as at least three letters' width
    of the name (about 13 sp × 3) fits beside it; otherwise the name goes
    over "· share as a code", the name still cut to one line and the rest
    wrapping, never clipped.

## Round 8: texts

262. **The texts the owner named are gone.** Contacts' list footer
    ("Contacts live in this wallet's file, on this phone only. Monero
    doesn't say who paid you, so contacts are for sending.", under Paid,
    not saved); Receive's "The code names you as “Henry” · change in
    Settings" row under the note, with `profile::receive_line`,
    `Wallet::receive_name_line` and its FFI method (nothing else used
    them; Your profile's "Name in payment requests" switch still says
    whether the name goes out); Receive's "The payer's Fob shows this
    pattern too" beside the address pattern and "Address N · a new one
    each time" / "Address N · this request's own" over the address, so New
    address now sits alone at the row's end. The pattern itself stays
    where it was until step 3 moves it into the code.
263. **Split up's footer is the network fee, and the price's age only
    when it's old**: "Network fee about $0.03", or "Network fee about
    $0.03 · 23 h old price" once the price has missed a refresh; nothing
    about coins being set in XMR, and no age in XMR. `split::TODAY` went;
    `split::view` takes the age (`price::alert::old`) rather than the
    "the 09:00 price · 23 h old" words.
264. **Node's "Sends blocks to Fob" puts the link speed on its own line**
    under the blocks per second (decision 1 of the plan): a check's row
    has a `second` line, set only for this check ("31 blocks/s" over
    "1.8 MB/s"), and the Node screen draws it under the result, right
    aligned, in the result's colour. TalkBack hears "Sends blocks to Fob:
    31 blocks/s, 1.8 MB/s". The sync details card already had Blocks and
    Link speed as rows of their own, so it's unchanged.
265. **The sweep removed what explains Fob, reassures, or says what the
    other side sees where the labels already say enough** (step 1):
    - Receive's Who is this for? sheet: "Pick a contact: this request and
      its address go on their card, and what arrives shows as from them."
    - The full-screen payment code: "Screen brightness turned up while
      this is open".
    - A payment's receipt (Settings › payment detail): "Open Receipt
      Format, from the shop's payment code. Kept on this phone with the
      payment."
    - Settings: Contacts' line "Who you pay, kept on this phone" became
      "Who you pay"; Use mainnet's "Real money. Each network has its own
      wallet." became "Real money"; Receipts' "Receipts from payment codes
      that carry one, on this phone only." lost ", on this phone only".
    - Instant updates: "Your node tells Fob about new blocks and payments
      the moment it sees them. Fob still checks every two minutes in case
      one is missed. Only your node's host is ever contacted."
    - Send: the note's placeholder "What it's for, kept on this phone"
      became "What it's for"; Paste an address's "Read from the clipboard
      only when you tap".
    - The photo sheet (Contact photo and Your photo): "Cropped to a circle
      and kept small, inside this wallet's file on this phone. Fob never
      sends it anywhere." and "Cropped to a circle and kept small, in this
      wallet's file. It leaves the phone only in your profile's code, when
      you share it." (`PhotoSheet` lost its footnote.)
    - Edit your profile: the intro's "They go in your payment codes,
      nowhere else." (the first sentence stays) and the name's help
      "Anything you like; it doesn't have to be your real name. Change it
      any time in Settings."
    - Your profile: Name in payment requests' line "Requests carry your
      name; only this profile code carries the photo", and "each its own
      address" beside Your shares.
    - Share my profile: "A new address for this share, never used again."
    - Back up your wallet: "A 16-word Polyseed: it also remembers when the
      wallet was made, so restoring is quick."
    - View-only wallet: "Fob will see payments to this wallet but can't
      spend them." (the way's own line, "Sees payments, can't spend",
      says it).
    - Wallet code found: "Fob keeps the keys encrypted on this phone,
      behind the Android Keystore."
    - Home's sync details while catching up: "You can still receive and
      send while Fob catches up." / "You can still receive while Fob
      catches up." (the failing note "Fob keeps trying on its own." stays;
      `catch_up::details` no longer needs `can_spend`).
    - Backup and keys › Keys: Main address's "Your first address; Fob
      gives out new ones each time."
266. **Kept for the owner to judge** (warnings, errors, empty states and
    intros a screen needs stay; these were close calls): the welcome
    screen's pitch and three points (its job is to explain), Restore's
    "Scanning its code is quickest.", Receive's note placeholder "What
    it's for, shown to the payer" (the note does go out), Coins' intro,
    Price from's intro, the price transport lines ("The provider sees this
    phone's IP address"), Backup and keys' "Fob hides them from
    screenshots …" and "The seed phrase is all you need …", the key lines
    (restore height, secret spend and view keys), Your shares' empty
    state, a share's Activity line "A new address for this share · 09:15",
    Home's "This wallet only watches …", the network switch dialogs'
    "… stays here" sentences, and Wallet code's "Scan it in Fob, the
    Monero GUI or Cake Wallet.". Show first's "Tap the balance to swap any
    time" is left for step 5, which removes Show first.
267. **Edge buttons: one size and inset** (round 8 › step 2, item 2). The
    numbers live in `EdgeButton` in ui/Components.kt and every edge button
    uses them through `EdgeIconButton` and `TopBar`:
    - an icon-only button is a 56 dp touch circle with a 26 dp icon (was
      Material's 48 dp `IconButton` or a 48 dp circle, with 20–22 dp
      icons);
    - its row sits 16 dp in from the screen's edge, so the outermost
      button's centre is 44 dp from the edge (was 28 dp in top bars, Pay's
      and Share my profile's full screen codes and Touch phones; 32 dp on
      Home and the scanners). Back on the left mirrors it;
    - Home's photo is 40 dp inside its 56 dp button (was 32 in 48), the
      Settings cog the same 56/26 as every icon action;
    - text actions ("Edit", "Clear", "Skip", "Save" and every
      `TextButton`) are at least 48 dp tall (were about 44) and end at the
      same 16 dp inset;
    - a top bar's title keeps to one line and ellipsizes, so a long title
      or a 200 % font never squeezes the buttons below their 56 dp. The
      bar grows past 64 dp only if the title's line needs it.
    Covered: every `TopBar` (Back, Close, Node's refresh, Contacts' scan and
    add, the Edit/Clear/Save text actions, Restore's scan screen), Home's
    photo and cog, Send's scanner Close and torch, Contacts' address
    scanner, Scan a profile, Receive's full screen code, Share my profile's
    code and Touch phones, and Edit your profile's onboarding Skip row. Not
    edge buttons, left as they were: the search field's clear, the price
    toast's button, Backup's copy buttons and Home's sync chip.
268. **Receive's code: the pattern in its middle, at high correction**
    (round 8 › step 3, item 5). Payment codes (Receive's card, its full
    screen code, and a request's "Show the code again") are made by
    `qr::encode_payment` (FFI `payment_qr_code`); Rust picks the level and
    the cleared square, Kotlin only draws:
    - error correction **H** while the code stays at version 25 or below,
      else **Q** up to version 30, else as any other code (M, or L past
      version 20) with nothing cleared. Measured with a real 95-character
      address: bare v10, with an amount v10, amount and short note v12, a
      request with a name and note v14, a 120-letter note v17, Receive's
      longest (integrated address, largest amount, 120 emoji note, 521
      bytes) v25 at H, a request's longest (40 emoji name too, 886 bytes)
      v29 at Q. Every payment code Fob makes gets the pattern. This replaces
      M for payment codes (where the code module said "fewer, larger modules
      read better"): the correction pays for the hole. The usual code goes
      from about v7 at M to v10 at H, 57 modules, still about 3.6 dp a
      module on the 220 dp card;
    - the cleared square is the largest odd side at most a quarter of the
      code's (v10: 13 of 57; v25: 29 of 117), exactly centred and aligned
      to whole modules: under 7 % of the modules, about a tenth of the
      codewords at most, against about 30 % (H) or 25 % (Q) recoverable.
      Its outer ring is a one-module light margin; the pattern's 5 × 5 dots
      fill the `centre − 2` modules inside, on the code's white (the card's
      colour in both themes), without the round badge and ring it has
      elsewhere. The square covers the middle alignment pattern of v7 and
      up, as logos in codes do; decoders locate by the corner one;
    - tested: `rqrr` reads every one of those codes rendered at 4 px a
      module, both as drawn (the square light) and with every module in the
      square the opposite of the code's own (the worst a pattern could
      read as); with the square at 60 % of the side the same test fails, so
      it tells. `rqrr` 0.11.0 (2026-09-02) is a dev-dependency only, with
      `lru` 0.18.5 (2026-09-23), `g2p` 1.2.2 and `hashbrown` 0.17.1 under it;
    - the separate pattern row under Receive's card and under the full
      screen code is gone, so Receive is 56 dp shorter (36 dp row and its
      20 dp gap). TalkBack reads the code as before ("Payment code"); the
      dots inside it are part of the drawing, not a separate item;
    - not changed: Share my profile's code (a dense v27-or-so code with a
      photo, at L, where a hole would cost too much; it shows no pattern,
      as before) and Backup's wallet code (`qr_code`, M, no
      pattern; it isn't an address to pay). A request's own screen keeps
      its small pattern beside the address row.
269. **Coins: Split up greyed out until a coin is ready** (round 8 › step
    4, item 6). `Coins.can_split` (fob-core `not_ready::coins`, FFI
    `Coins.canSplit`) is true when some coin is ready to spend (the ready
    sum above zero); `Wallet::coins` also clears it for a view-only
    wallet. Kotlin passes it to `PrimaryButton(enabled = …)`; with no
    coins, or only coins not ready yet, Split up stays in its place in
    `PrimaryButton`'s disabled style (sunken fill, muted words and now a
    muted icon too, where the icon stayed dark before) and can't be
    pressed. Until the Coins view has loaded (it arrives a frame late
    until step 7) the button is drawn disabled, so it never flashes
    pressable. A view-only wallet still shows no Split up at all, as
    before: it can never split, so a button greyed out for good would only
    be noise. Not part of the condition: whether the ready coins cover a
    note and the fee; Split up's own screen says so when they don't.
270. **One stronger warning style** (round 8 › step 4, item 11). A new
    `Warning` composable in ui/Components.kt and a new theme role
    `Fob.warning` (light `#F7D6B4`, dark `#46301A`) replace the
    `Fob.highlight` rows each screen drew itself. Words are ink (not
    muted) at body size, 15 sp on 21 sp, the warning mark 24 dp in
    `orangeText`; no edge, no outline, 18 dp corners. Used by: Backup and
    keys' "Anyone who sees these …", Seed phrase's "These N words are your
    wallet …" (was muted text with no block), Wallet code's two warnings,
    Home's backup reminder (title, a line now in ink at body size instead
    of muted 13 sp, its chevron), the "Use mainnet?" and "Remove this
    wallet?" dialogs' text, a contact's "replaces …" address warning, and
    Restore's scan screen, which keeps the camera's colours
    (`cameraCard`, `onCamera`, `cameraWarning`) through the same
    composable. Keys has no warning of its own (the hub's covers it).
    Left on `highlight`, as they aren't warnings: Split up's tip (step 8
    reworks it), the Stagenet and Quickest badges, Profile's ring.
    Contrast (WCAG): light ink `#1A1917` on `#F7D6B4` 12.74:1, the mark
    `#B34700` 3.99:1 (graphics need 3:1); dark ink `#F2EEE6` on `#46301A`
    10.69:1, the mark `#FF8F45` 5.46:1; the camera's `#F4F1EA` on
    `#3A3732` 10.50:1, mark 5.05:1. Against the page the surface went from
    1.03:1 (light) and 1.22:1 (dark) to 1.22:1 and 1.50:1 (on a card or
    dialog 1.10 → 1.38 and 1.11 → 1.36), so the block now reads as a
    block. The disabled Split up: muted on sunken 6.10:1 light, 6.29:1
    dark.
271. **Default currency, a short value** (round 8 › step 5, items 9 and
    10). Settings' "Also show amounts in" is now "Default currency"
    (sentence case), and its list screen's title too. The line under the
    title ("Uses today's XMR price" / "Fob never looks up a price") is
    gone, so the row is one line; Rust's `price::default_currency_text`
    gives the value: code and symbol ("AUD · A$", "USD · $"), the code
    alone without a symbol ("CHF") or with a symbol equal to the code,
    "XMR" for XMR. Never the full name on the row; TalkBack hears it
    instead (`default_currency_name`: "Australian dollar", "Monero
    (XMR)"). The longest of the 45 values is 10 characters, tested. The
    value is one line, ellipsized, and at a big font still drops under
    the title (`TitleAndValue`) rather than squeezing it. Price from stays
    the row under it with a currency.
272. **The list's XMR row is "Monero (XMR)"** in a group titled "MONERO"
    (was "XMR only" under "NONE", which read oddly once the setting is a
    default currency). Its line stays "Fob never looks up a price". A
    search for "xmr", "monero" or "ɱ" finds it. Price from's intro with
    XMR now says "With XMR as the default currency, Fob never looks up a
    price."
273. **Show first goes; the swap stays remembered.** The Settings row
    and its "Tap the balance to swap any time" are gone, and with them
    the `Segmented` control (only that row used it) and
    `PriceSettings.fiat_first`/`currency_line`. The wallet file keeps
    `prices.fiat_first` under the same name, so a wallet saved with Show
    first on its currency still leads with it (tested from a file, and a
    file without the field opens XMR first). Home's balance tap still
    toggles it. Kept while XMR is chosen, so picking a currency again
    brings the order back; Rust refuses the toggle with XMR.
274. **Rust decides the swap: `Balance.can_swap`.** `price::balance_order`
    (via `Wallet::balance_order`) is true only with a default currency
    besides XMR and its worth to show (a price within the day), and
    `Balance.fiat_first` is false whenever it can't swap. Home's balance
    is only clickable (and only announces "Swap XMR and AUD") when
    `canSwap`; with XMR it is plain text, not a disabled button. The
    amount fields' chip (Send, Receive, Pay) was already Rust's
    `can_switch`, false with XMR (`FieldPrice::None`), so it shows on no
    screen. There is no other ⇅ control in the app.
275. **Amount fields don't follow Home's order.** The fields keep
    starting in the default currency whenever there is a price, as
    before; a chip tap switches that field only. `fiat_first` stays
    Home's balance order alone, so the swap is "remembered as before"
    without one field's tap reordering the balance.
276. **Backup and keys is the one place that warns** (round 8 › step 6,
    item 12). Its warning and the phone's lock come first; what it opens
    shows its secret straight away:
    - Wallet code: no warning block ("This code holds the keys to spend
      your money …" / "This code lets anyone see this wallet's
      payments.") and no Show the code / Hide step; the code draws as soon
      as Rust returns it, with "Scan it in Fob, the Monero GUI or Cake
      Wallet." under it (kept by 266). Both texts were Kotlin's, so no
      Rust text or function goes; `can_spend` is still used by Home and
      Coins.
    - Seed phrase from the hub: no "These 16 words are your wallet …"
      block above the words.
    - Keys already had neither (round 7), so it is unchanged.
    - None of them asks for the lock again; FLAG_SECURE (`NoScreenshots`)
      and the onStop cut (now a `locked()` check, as `ShowSeed` became a
      data class) are as before.
277. **The other ways to the seed phrase keep their warning.** Wallet
    code and Keys are reachable only from the hub. The seed phrase has
    two more ways: Home's backup reminder (`Screen.ShowSeed(warn = true)`,
    after the phone's lock, as before) and a new wallet's "Back up your
    wallet" step (`Screen.Backup`, no lock: the wallet was just made).
    Both keep the warning block, since nothing before them showed one;
    the reminder card is a call to act, not the warning about who can use
    the words. The mainnet switch shows no secret, so it has nothing to
    change.

## Round 8: layout shifts

Found on the QA emulator by screen recording (frames in the round's job
folder, `shift/` before and `shift/after/` after). The common cause: a
screen started its Rust-backed state empty and filled it from an async
UniFFI call a frame after its first draw, and lines that come and go
pushed what was under them.

278. **The screens' data lives in WalletModel, read with the wallet.**
    `refresh()` (on open, after every change and on every sync status)
    now also reads Home's backup reminder, Settings' Node and Backup and
    keys lines, Receive's own address and index, Contacts' list and
    "Paid, not saved", Send's folded sheet (contacts on Send, any
    contact, paid before) and Coins, and sets everything at once after
    reading it all (never one answer new and the next old), dropping
    answers from a wallet closed meanwhile. Settings' profile row is the
    model's `you`, already there. Each screen draws whole on its first
    frame. The model's one-shot readers these replaced (`coins()`,
    `contactsOnSend()`, `paidBefore()`, `paidNotSaved()`, and
    `contacts()` without a search) are gone; Contacts' search still asks
    Rust as it's typed. Settings reads its two lines again as it opens,
    for a Node check or a screen lock set meanwhile; New address and "I've
    written them down" go through the model so its copy stays right.
279. **Receive's address is handed out with the wallet's refresh.** Rust's
    `receive_address` hands out a new own address when the last went to
    a request, a split or a share; it now runs in `refresh()`, so a new
    wallet's first Receive address is handed out as it opens rather than
    when Receive first opens. Nothing is shown or sent by it.
280. **The launch screen stays up while the saved wallet opens.** No
    splash library is in the tree, so MainActivity holds its first draw
    (a `ViewTreeObserver.OnPreDrawListener` returning false, Android's
    own way, which keeps the system splash on Android 12+) while
    `WalletModel.opening` (a saved wallet, no balance read yet, no open
    error), at most 2.5 s. Home's first frame has the balance, photo and
    reminder, then fades in with the splash's exit.
281. **Lines that come and go keep their room.** Home's "Balance so far"
    is always laid out and fades out once synced (TalkBack skips it then),
    so the balance and everything under it stay put as syncing ends;
    likewise "Still finding older ones" while there's activity. The
    amount field's line under it is always laid out, empty when Rust has
    nothing to say. Receive opened for a contact keeps the chip's 48 dp
    while Rust reads them. The synced Home is ~15 dp taller at the top
    than before, the label's empty room.
282. **Amount fields open whole, from Rust.** `PriceSettings.blank_entry`
    is the blank field as a form opens (`price::blank_field`: in money at
    the shown price, else XMR; `Wallet::blank_amount_field`, which asks
    for no price, unlike `amount_field`, so a refresh off screen never
    triggers a lookup). Send and Receive start from it: sign, chip and
    rate line on the first frame, no two round trips. A price that comes
    later switches the field to money with its words in the same step.
283. **Send's camera overlay waits for the sheet's measure.** The
    viewfinder, its words and Allow the camera are placed by the sheet's
    measured height; until it's measured (one or two frames) they're
    drawn at alpha 0 rather than at a guess that jumped ~50 dp. Pulling
    the sheet up no longer flashes the form at full height for a frame
    before the settle starts: until the settle's first frame the sheet
    keeps the stop it was shown at.

## Round 8: Split up Auto

284. **Split up opens empty, with Auto only when there's something to
    learn.** The owner picked design A. `split::suggest` and its
    `Suggestion` (counts and the "Picked from your last 30 days: most
    payments were …" or "Not enough payments yet …" words) are gone with
    `Wallet::split_suggestion` and the FFI's `SplitSuggestion`; nothing
    else used them. The even mix (12 pieces over three neighbouring notes)
    and its `EVEN_PIECES` went with it, and its test; the percentile range
    that named "most payments" went with the words. What's left is
    `split::learned`, today's learned branch unchanged: with 3 or more
    payments (`ENOUGH_PAYMENTS`) in the last 30 days, the mix filled in
    their proportions up to 15 coins or the money. Its tests were ported:
    the coffee-and-lunch mix, payments that agree, one bigger than every
    note.
285. **Rust says whether Auto is offered, its mix and whether it's lit.**
    `split::View` (and the FFI's `SplitView`) carries `auto:
    Option<Auto>` (the counts, "Auto", and TalkBack's "Auto, fill coins
    like your last 30 days") and `is_auto`, true while the counts are
    Auto's mix (compared after sizing, so a list without its trailing
    zeros still matches). `split::view` takes the payments and works it
    out; `Wallet::split_view` reads them for the 30 days to the phone's
    clock, the same filter as before (sent, not a split, not empty).
    Kotlin starts with no counts, draws `auto`, sets the counts to
    `auto.counts` on a tap and draws it lit from `is_auto`.
286. **No Auto when not one coin fits.** With enough payments but too
    little ready for even the smallest note and the fee, `learned` is
    `None`: a button that filled nothing would be lit for an empty screen.
    The board doesn't draw this case; it reads like the few-payments one,
    no Auto and no words.
287. **Auto's look.** A pill in the top bar before Clear, 48 dp tall (step
    2's text-action height), a filled sparkle in `orangeText` and "Auto"
    in ink at 15 sp semibold. Lit: a new `selectedFill` role (light
    #FBE3CF as the board, dark #452812, the warm orange tint at night) and
    a 1 dp `orangeText` edge; plain: no fill and a 1 dp `outline` edge.
    TalkBack reads Rust's sentence with the selected state while lit. A
    tap while lit sets the same counts, so nothing changes. The sparkle is
    the board's path drawn filled, through a new `FilledIcon` beside
    `StrokeIcon`.
288. **Clear shows only with a coin chosen.** It comes in beside Auto as
    the first coin is added and goes when cleared; Auto moves left by
    Clear's width then, as the board draws it.
