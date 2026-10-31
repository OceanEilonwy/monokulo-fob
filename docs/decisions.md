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
16. **Background sync**: the wallet syncs while the app is on screen. A
    foreground service with a progress notification comes next.
17. **Google Wallet**: not built. Each pass is a JWT signed with a Google
    Wallet issuer's key, which can't ship in the app, so it needs an issuer
    account and a small signing service. Settings says so.
18. **Fiat amounts**: none yet. Fetching rates means asking a price service,
    which tells it when you use your wallet.
19. **Subaddresses**: account 0 only, 50 watched past the last one handed out.
20. **ABIs**: arm64-v8a (phones) and x86_64 (emulator) only.
21. **Pipelining across sync steps**: each step waits for its slowest
    request. Keeping requests in flight between steps would add 20–30 %.
22. **Untested on chain**: sending needs coins. Scanning, restore and the
    whole app run against real stagenet nodes; a send has not been made yet.
