# Proposal: a Light Wallet mode for Fob

Status: proposal, 2026-11-07. Phase 2, read-only light mode, is built in Fob
(decisions 371–376) and hardened (377–382: amounts held to the server's
commitments, fees, time locks, the client's rules; 384–387: each payment
proved to be in a block a node has); phase 3, sending through the server,
is built (389–394); the server half (`monokulo-lws`) exists as a first
working version, in its own repository.

## What it is

A switch in Settings › Node: **Sync through a light wallet server**. With it
on, Fob stops fetching blocks and scanning them itself. A server that holds
the wallet's *view* key does the scanning, continuously, and Fob asks it what
the wallet received and which of its outputs may have been spent. The spend
key never leaves the phone: Fob still computes key images, chooses what to
spend, builds and signs every transaction locally, and only hands the server
the finished transaction to broadcast.

The protocol is the Monero **light wallet REST API** (monero-project/meta,
`api/lightwallet_rest.md`), the one MyMonero defined and
[monero-lws](https://github.com/vtnerd/monero-lws) implements, with
monero-lws's additions for subaddresses. Fob would work with any server that
speaks it: monero-lws, OpenMonero, and `monokulo-lws`, the Rust server built
alongside this proposal.

## Why

- **Opening Fob is instant.** Today a phone away for a week fetches and
  scans about 5,000 blocks (10–35 s on stagenet, minutes of mainnet
  bandwidth). A light wallet opens with one request a few kilobytes long,
  however long it was away. Fob's whole fast-sync machinery (decisions 6–9,
  30–35) exists to make catching up bearable; a server makes it unnecessary.
- **Data and battery.** A day of mainnet is tens of megabytes of blocks
  (decision 8). Light mode is a few kilobytes a poll, and nothing to scan.
- **Monokular.** A shop running Monokular can run one `monokulo-lws` for its
  tills and the owner's Fob, next to the node it already runs. The server
  also gives the till side something it has wanted: webhooks when a payment
  arrives, without each till scanning.
- **Desktop and other clients** get the same mode for free, since it lives
  in the Rust crates.

The price is privacy, and it has to be said plainly in the UI: the server
learns everything the view key reveals, which is every payment received, its
amount, its subaddress and its time, plus which of the wallet's outputs
appear in rings (candidate spends), and the phone's address each time it
asks. It cannot spend. This is the same trade MyMonero and Cake's light
servers make. Fob should treat it like instant updates over ZMQ (decision
45): off by default, chosen per wallet, with the server named by the user.

## What the protocol gives, and what it doesn't

Every call is a `POST` of JSON to `/<method>`; the address and private view
key are the credential (there is no other auth). Amounts travel as decimal
strings, keys as hex.

| Call | What Fob gets |
|---|---|
| `/login` | Registers or recognises the account. `new_address`, `start_height`. 501 if the server doesn't take new accounts; 403 while an admin hasn't approved yet. |
| `/get_address_txs` | Every transaction that touched the wallet: hash, height, time, `total_received`, `total_sent`, `unlock_time`, `coinbase`, `mixin`, `payment_id`, `fee` (monero-lws, and monokulo-lws from `0fca96c`'s wire types), and per transaction the **candidate spends** (`spent_outputs`: amount, key image, the spent output's tx pub key and index, ring size) and, from monero-lws on, the **outputs received** (`receives`: amount, one-time key, index, global index, tx pub key, RingCT mask, subaddress, and from monokulo-lws the output's on-chain `commitment`). From monokulo-lws (`3bb52de`'s wire types), a mined transaction's **`proof`**: its block as Monero serialises it and its pruned parts. Plus `scanned_height`, `blockchain_height`. |
| `/get_address_info` | The same heights, `locked_funds`, totals and all candidate spends, without the transactions. |
| `/get_unspent_outs` | Every output received, with its global index, the 96-byte `rct` blob (commitment, then mask and amount encrypted the old way), tx hash, tx prefix hash, the key images of every candidate spend of it, and the network fee (`per_byte_fee`, `fee_mask`, and per-priority `fees`). |
| `/get_random_outs` | Ring members: `count` outputs per requested amount (`"0"` for RingCT), each with global index, one-time key and commitment. The **server** picks them (gamma over age, as wallet2). |
| `/submit_raw_tx` | Broadcasts the transaction. `status`, `reason`, `not_relayed`. |
| `/import_wallet_request` | Asks for a scan from block 0 (a restore). May answer with a fee to pay first. |
| `/get_subaddrs`, `/upsert_subaddrs`, `/provision_subaddrs` | monero-lws's extension: tell the server which subaddress indices to watch; it answers which were new. Servers may cap the count or have it off. |
| `/get_version` | Server type, API level, network, `max_subaddresses`, chain height. |

What the protocol leaves to the client, and what it lacks:

- **Spends are candidates only.** The server sees an input whose ring holds
  one of the wallet's outputs; only the spend key tells if that is the real
  spend. Fob computes the key image of the named output and compares. Fob
  already keeps the spend key resident for exactly this (the auth-bound
  wrapper was rejected on 2026-11-07 for the same reason).
- **The mempool.** The community API has a `mempool` flag but monero-lws
  never reports pool transactions over REST (its docs say so; webhooks and
  the `/feed` websocket do). `monokulo-lws` does: a pool transaction to the
  wallet appears in `/get_address_txs` with `mempool: true` and no height,
  so Fob's "on the way" rows (decision 15) keep working against it. Against
  monero-lws they simply appear when mined.
- **No block hashes, no reorg signal.** Fob can't anchor to recent block
  hashes as it does now (decision 9). Instead each poll is a *reconciliation*:
  the server's list of transactions is the truth, and a transaction that was
  there and is gone (reorged out) has its outputs removed. Fob never scans
  in light mode, so this is cheap and idempotent. The blocks a proving
  server's transactions are in (decision 384) are held to a node's hashes,
  which Fob keeps with a few near where it last asked, so a reorganisation
  there shows and those blocks are checked again (decision 385).
- **No output distribution.** Fob's decoy cache (decision 27) and
  monero-oxide's own decoy picker need `get_output_distribution`, which the
  API doesn't have. In light mode the server picks decoys, as the protocol
  intends. See *Sending*.
- **Subaddresses need registering.** Fob watches account 0, minors 1 to
  `receive_index + 200` (decision 19, `LOOKAHEAD`). It must tell the server
  that range through `/upsert_subaddrs`, and again whenever it grows. A
  server with subaddresses off (409, `max_subaddresses: 0`) would miss every
  request and split (decisions 76, 116), so Fob should refuse to turn light
  mode on against one, with words saying why. `monokulo-lws` also watches
  minors 0–199 of account 0 for every account without being asked (its
  `lookahead`), so a Fob that hasn't registered yet misses nothing.
- **Timelocks.** The server reports `unlock_time` per transaction; Fob keeps
  outputs locked to a block or to a time as the node path does (decision
  380), judging a time by the latest block time the server vouches for,
  never one past the phone's clock.
- **Instant updates.** monero-lws has a `/feed` websocket (login, then a
  stream of `blocks:`, `mempool:`, `warning:` with reorg heights). It is the
  light-mode counterpart of ZMQ (decision 44) and a later phase.

## Where it goes in the crates

Fob is split by what each part may touch (decision 339), and light mode
fits that split without a new layer:

- **`fob-chain`** gains `light/`: the wire types (`lws.rs`: serde structs for
  every request and response, hex and decimal-string helpers) and
  `LightClient`, an async client over the same reqwest-and-rustls transport
  the node uses (`transport.rs`: self-signed certificate pinning, Basic auth
  from the URL, no redirects, the onion refusal of decision 73, the 30 s idle
  limit of decision 28). The client is small: one `post(route, &T) -> U`,
  and a method per call. `monokulo-lws` publishes its wire types as the
  `lws-api` crate; whether Fob depends on it or copies the file is a choice
  (below).
- **`fob-chain::node::Setting`** stays the node. A new `Backend` beside it:

  ```rust
  pub enum Backend {
      Node,                                   // today's path
      Light { url: String, self_signed: bool, registered: bool },
  }
  ```

  kept in the wallet file beside the node and the ZMQ setting: `wallet` table
  columns `sync_mode` ('node' | 'light'), `light_url`, `light_self_signed`,
  `light_registered`, `light_subaddrs_sent` (the highest minor told to the
  server). This is the first change to the SQLCipher schema since version 5,
  and `db.rs` today refuses a version it doesn't know rather than migrating.
  It needs a migration step: `ALTER TABLE wallet ADD COLUMN ... DEFAULT ...`
  for each, then bump `VERSION` to 6. Old files open as node wallets.
- **`fob-wallet/src/wallet/sync.rs`**: `sync_step` branches at the top:

  ```rust
  match self.backend().await {
      Backend::Node => self.node_step().await,      // the body as it is today
      Backend::Light { .. } => self.light_step().await,
  }
  ```

  The loop around it (`start_sync`, `wait`, `sync_now`, backoff, the 20 s
  poll, `SyncListener`, `SyncStatus`) is unchanged. ZMQ is reported Off in
  light mode. Manual Sync (`manual.rs`, decision 35's successors) is moot in
  light mode, since the server scans regardless; the card hides, and "Check
  now" is a poll.
- **`fob-wallet/src/wallet/send.rs`**: `prepare_payments` takes its decoys,
  fee and tip from a `SendSource` trait with two implementations, the node's
  (what `SendContext` and `Decoys` do today) and the light client's.
- **`fob-ffi`**: the `NodeSetting` pattern repeats: a `LightSetting` record,
  `light_server()`, `set_light_server(url, self_signed)`, `check_light_server`
  with a `NodeReport`-like result, `use_light_wallet(on)`. `SyncStatus` grows
  nothing; the chip words come from `fob-words` as they do now.
- **Android** (`ui/screens/Node.kt`, `Settings.kt`): the Node card gets a
  segmented "Sync through: Node | Light wallet server" and a Light server
  screen (URL, self-signed, Test, the privacy words). `fob-gtk` the same.

## The light sync step

```
light_step():
  1. ensure registered:
       /login { create_account: true, generated_locally: !restored }
       403  -> status "Waiting for the server to approve this wallet" (retry on poll)
       501  -> error "This server doesn't take new wallets"
       ok   -> registered = true; if the server's start_height is above our
               restore height: /import_wallet_request, and show its status
               ("The server is scanning from <date>", or the fee it asks)
  2. subaddresses: if watched() > light_subaddrs_sent:
       /upsert_subaddrs { 0: [[1, watched()]] }; 409 -> error (see above)
  3. /get_address_txs -> reconcile(state, response)
  4. status: scanned = scanned_height, tip = blockchain_height,
     synced = scanned_height + 1 >= blockchain_height
  5. prices: record_prices for newly seen payments, as the node step does
```

`reconcile` is the heart, and the only new logic of any size:

- **Received.** For each transaction's `receives` (or, on a server without
  that field, from `/get_unspent_outs`), an output not yet in `state.outputs`
  becomes an `Owned`. Fob needs a monero-oxide `WalletOutput` for each coin
  (`Owned.output`, read back with `WalletOutput::read` when spending).
  `WalletOutput` has no public constructor, but its serialised form is
  simple and `read` is public, so Fob writes the bytes itself:

  ```
  tx hash (32) ‖ index in tx (u64 LE) ‖ global index (u64 LE)
  ‖ one-time key (32) ‖ key offset (32) ‖ mask (32) ‖ amount (u64 LE)
  ‖ timelock ‖ subaddress? (1 + 4 + 4) ‖ payment id? ‖ arbitrary data count
  ```

  where `key offset = H_s(8·a·R ‖ i)`, plus `H_s("SubAddr\0" ‖ a ‖ major ‖
  minor)` for a subaddress, and `mask = H_s("commitment_mask" ‖ H_s(8·a·R ‖
  i))` for modern transactions (or the mask decrypted from the `rct` blob).
  Fob must **check** before trusting: `one-time key == key offset·G +
  spend key (of that subaddress)`, and `amount·H + mask·G == the commitment`
  the server sent (decision 377). A server can hide outputs. The checks catch
  an output of another wallet's and an amount that doesn't fit the
  commitment given; but the commitment alone is the server's word for the
  chain, and a server holding the view key can make a key and a commitment
  that fit an output it invented. So a server that can proves each
  transaction is in a block (decision 384): Fob works out the transaction's
  hash from its pruned parts, finds it in the block, reads the one-time
  keys, commitments, fee and unlock time back from the parts, and holds the
  block's hash to a node's at that height, asked over stretches of the
  chain so the node can't tell which blocks are the wallet's (decision
  385). The format is "not protocol defined" (monero-oxide's
  words), so this needs a round-trip test pinned to `monero-wallet 0.2`, and
  a small upstream PR for a `WalletOutput::new` would be the lasting fix.
- **Spent.** For each candidate in `spent_outputs`, find the owned output by
  (`tx_pub_key`, `out_index`), compute its key image (`keys.rs::key_image`,
  as `take_block` does today), and if it equals the candidate's, mark the
  output `spent_in` that transaction and push a `ChainSpend` (fee from the
  transaction; 0, shown as not known, when the server gives none: decision
  379). A view-only wallet can't, and shows what it shows today.
- **Sent by this phone.** A transaction whose hash matches a `Sent` gets its
  height, as `take_block` does.
- **Gone.** A transaction Fob recorded with a height that the server no
  longer lists was reorged out: drop its outputs, un-spend inputs it spent,
  clear its `Sent.height`, as `rewind` does for a height range.
- **Pool.** Transactions with `mempool: true` fill `pool` (memory only, as
  today); their outputs are not `Owned` until mined.
- **Burning bug** (decision 23): the first output with a given one-time key
  wins, as today; the server also keeps only the first.

## Sending

Built (decisions 389–394). `prepare_payments` keeps its shape: pick coins
largest first, build a `SignableTransaction`, retry on `NotEnoughFunds`.
What changes is where the fee and each input's ring come from
(`wallet/light_send.rs`: a `Source` that is the node's or the server's):

- **Coins.** The wallet's own records, found and checked by light syncing
  (key, mask, commitment, proof; key images worked out here), ready by the
  same count of confirmations against the server's tip from the last poll.
  `/get_unspent_outs`' outputs aren't used: only its fee is.
- **Fee.** `/get_unspent_outs` gives `per_byte_fee`, `fee_mask` and the four
  priorities' `fees`. Fob pays the second, Normal, as monero-daemon-rpc picks
  it from a node's estimate for the node path; a server without `fees`
  (older monero-lws) gives `per_byte_fee`, the lowest. Refused above
  `MAX_FEE_PER_WEIGHT` (decision 12's cap), or with a mask above
  `MAX_FEE_MASK` (100,000; monerod's is 10,000), a cap the node path gains
  too, so a hostile server can't take a large fee (decision 389).
- **Decoys.** One `/get_random_outs { count: 16, amounts: ["0"] × inputs }`
  (150 rings a request at most, monero-lws's limit). monero-oxide's
  `OutputWithDecoys::new` picks its own decoys from the distribution, which
  the API lacks; but `OutputWithDecoys::read` is public and its format is
  `OutputData ‖ Decoys` (`key ‖ key offset ‖ mask ‖ amount` then
  `monero_clsag::Decoys`, whose `Decoys::new(offsets, signer_index, ring)` is
  public). So Fob builds each input from 15 of the server's 16 members plus
  its own output: the member that is the real output (if the server
  happened to pick it), or one at random, goes; sort by global index, the
  real one's position as `signer_index`, offsets as differences, ring as
  `[key, commitment]` pairs, the commitment the first 32 bytes of `rct`.
  Checks: RingCT's amount, exactly 16 members, distinct, every key and
  commitment a point, a member at the real output's index the real output;
  members with torsion or the identity dropped as monero-oxide drops them.
  Given monero-oxide's own picks, the ring is its byte for byte (a test).
  Not checked: the members' age, as the API gives no heights. A server
  already knows which output is ours, and learns which is spent when the
  ring is formed (the member it didn't hand out); decoys hide the spend
  from the chain, not from it
  (decision 391).
- **Tip.** `blockchain_height` from the last poll.
- **Submit.** `/submit_raw_tx { tx: hex(tx.serialize()) }`. "OK" is
  published; "OK" with `not_relayed`, no answer, or a 5xx but 503 is recorded and
  offered again at each poll, as the node path does with a payment its node
  didn't answer for; a 422, 400, 503 or another status is a refusal, said with the
  server's reason (one line, 120 characters). The payment is recorded as sent
  at once, as today (decision 14), and the next poll shows it in the pool
  (monokulo-lws), then mined, with the server's fee for the spend.
- **Trezor.** Stays with the node: a device's key images are asked of a node
  (decision 393).

The decoy cache file (`.decoys`) isn't used or updated in light mode.

## The settings screen and the words

Settings › Node, with light mode on, reads "Light wallet server ·
lws.example.com". The screen:

- **Sync through**: Node | Light wallet server (one `Segmented`).
- **Server**: the URL (`https://host:port`, or `http://` only for a server on
  the phone itself, decision 381; no `.onion` or `.i2p`, decision 73), a
  self-signed toggle, and **Test**, which calls `/get_version` and reports
  the network (refused if it isn't the wallet's), the API level, whether
  subaddresses are on (refused if not), and the chain height against the
  node's.
- **Words, on the screen and before the switch turns on**: "The server gets
  this wallet's view key. It will see every payment you receive, how much,
  and when you open Fob. It can't spend. Choose a server you run or trust."
  And on turning it off: "Fob will scan on this phone again from where the
  server left off. The server still has the view key until you tell it to
  forget the wallet."
- **Status chip**: "Synced · through server", "Server is N blocks behind",
  "Waiting for the server to approve this wallet", "Can't reach the server".
- **Restore** with light mode already on (a wallet file with the setting
  opened on a new phone): `generated_locally: false` and an import request,
  with the server's answer shown. A server that charges an import fee shows
  the amount and address; Fob doesn't pay it on its own.

## What the server can and can't do to Fob

| It can | Fob's answer |
|---|---|
| See every payment in, with amounts, subaddresses and times. | Said in the words; the user's choice. |
| Guess spends from which outputs sit in rings. | Statistical, same as any light wallet. |
| Hide a payment or a spend, or leave a proof off. | Not detectable; also the user's choice of server. Fob can at least show when the server's `scanned_height` lags the chain, and a payment without a proof makes the Light server screen say "commitments only" or "the server's word" (decision 386). |
| Name an output of another wallet's, or one under a key Fob can't spend. | Caught: the one-time key is re-derived from the transaction key, the view key and the index, and the mask from hard fork 10 on (decisions 373, 382). |
| Misstate an amount. | Caught when the server serves commitments (monokulo-lws): the amount and mask must open the commitment given, in constant time, or the whole answer is refused and the sync stops with words (decisions 377, 378). monero-lws serves none: its amounts are taken on its word, and the Light server screen says so. |
| Invent an output whole (it holds the view key, so key, mask and commitment can be made to fit). | Caught when the server proves its transactions (monokulo-lws) and a node answers: the proof must hold and name the block the node has at that height, or the whole answer is refused and the sync stops with words (decisions 384, 385). Without a proof (monero-lws) it isn't, and the screen says the payments are held to commitments only, or are the server's word; with the node unreachable, "not checked yet" until it answers. Such a coin can't be spent either way: the node refuses the transaction. |
| Misstate a proved transaction's fee, unlock time, time or height. | Caught: each is read back from the proof (decision 384). |
| Contradict itself (one output with two keys, two outputs at one global index, a block past the tip). | The whole answer is refused (decision 378). |
| Misstate the tip, or a height or global index it doesn't prove. | Not checked: confirmations count from the server's tip, and a global index is used only when spending, where the node checks it. A block far past the tip is refused (378). |
| Learn, through the node, which blocks are the wallet's. | It isn't the server's to learn, and the node isn't told: it's asked for stretches from where Fob last stopped, and only past 20,000 blocks for each payment's 1,000-block window, which leaks the week at most, not the block (decision 385). |
| Report a wrong network fee for sending. | Capped by `MAX_FEE_PER_WEIGHT`, and its rounding by `MAX_FEE_MASK` (decision 389). |
| Report a wrong fee for a spend found. | Shown as given, or "Not known" without one (decision 379); it moves money between a history row's amount and its fee, never the balance. |
| Time a time lock wrongly. | Judged by its block times, never one past the phone's clock: early only if the phone's clock is wrong too (decision 380). |
| Pick decoys that weaken a ring. | Checked for count, distinctness, being points and RingCT's; not for age (the API gives no heights); beyond that, the trade of the mode (decision 391). |
| Not broadcast a transaction. | A refusal is shown and nothing recorded; no answer or `not_relayed` records the payment and offers it again each poll, and the day-old check marks one never mined (decision 392). |
| Learn the phone's address and when it's on. | Said in the words. Tor is still future work (decision 73) and would cover this. |

Transport: TLS as for nodes, including a pinned self-signed certificate,
shown as its key's fingerprint and held to it from when the server is
chosen, compared in constant time; a changed key fails closed. The view key
is in every request body, so plain `http://` is refused but for a server on
the phone itself (decision 381). A request may take 120 s in all, an answer
32 MB.

## Phases

1. **Server** (done, first version): `monokulo-lws`, below. **1b. Server at
   scale**: the prefilter lifted into `monokulo-8ar` (done, decision 366),
   its keyring and per-item digits, `monokulo-lws` on `flag_many` (the
   section after it).
2. **Fob, read-only light mode** (done, decisions 371–376): `fob-chain::light`,
   the `Backend` setting and schema migration (version 8, from 7), `light_step`
   with `reconcile`, the Node screen and words, the chip. Sending in light mode (until phase 3)
   disabled with "Sending through a server comes next". Tested against
   `monokulo-lws` on stagenet, a faucet payment seen in the pool and then
   mined; not yet against monero-lws in Docker. Where it differs from this
   proposal: an import is asked only when blocks would go unscanned (372),
   the wire types are copied rather than depended on (374). Hardened since
   (377–382): amounts held to the commitment monokulo-lws serves, and said
   to be the server's word where it serves none (monero-lws); an untrue
   answer refused whole; the server's fee for a spend found, "not known"
   without one; time-locked outputs kept as the node path keeps them;
   `https://` only but for the phone itself, a 120 s request limit, the
   pinned key shown on the Light server screen. Then (384–387) a proof of
   inclusion for each mined transaction, checked against block hashes from
   the wallet's node (or Fob's default) asked over stretches, kept so a
   poll asks nothing twice, and checked again after a reorganisation; the
   Light server screen says whether payments match the chain, are held to
   commitments only, or are the server's word.
3. **Sending** (done, decisions 389–394): a `Source` for the node or the
   light client, rings from the server's members, the fee from
   `/get_unspent_outs` capped, `/submit_raw_tx`. Tested end to end on
   stagenet. A Trezor wallet sends through its node.
4. **Instant updates**: `/feed` on both sides (monero-lws's websocket
   protocol, msgpack), reported in the chip as ZMQ is. Webhooks from
   `monokulo-lws` for Monokular tills.
5. **Later**: Tor through arti for both the node and the server; a Monokulo
   "connect code" that carries a server URL so a shop's Fob needs no typing.

## Decisions to make

- **Wire types: depend or copy.** `lws-api` is a protocol-only crate with no
  Monokulo in it, so depending on it from crates.io doesn't break decision
  5. Decision 44 copied ZMTP rather than share it. Either is fine; copying
  keeps Fob's dependency rule (decision 26) simple. Recommendation: depend,
  pinned, since the types will change as the protocol does.
- **Per wallet or per app.** Per wallet, in the wallet file, like the node:
  a stagenet and a mainnet wallet want different servers.
- **What happens to scanning state when switching.** Turning light mode on
  keeps `outputs` and `sent` and drops the block anchors; the first poll
  reconciles. Turning it off resumes node scanning from the server's
  `scanned_height` minus 30 blocks (the reorg margin of decision 9).
- **Whether to allow a server without subaddresses.** Recommended: no.
- **A `WalletOutput` constructor upstream** rather than writing its bytes.
  Worth a PR to monero-oxide either way.

## monokulo-lws

Repository: `monokulo-lws`, beside `monokulo-fob`. Rust, GPL-3.0-only. One
binary, two crates:

- `lws-api`: the wire types above, serde, usable by any client or server.
- `monokulo-lws`: the server. `serve` scans and answers; `admin` makes keys
  and accounts in the database directly.

What it does, as of this proposal:

- Scans with monero-oxide's `Scanner`, one scan per active account per
  block, from `get_blocks.bin` (the reader is Fob's, decision 6: it accepts
  the all-zero prunable hash public nodes send). Accounts behind the rest
  (new, or asked to rescan) are caught up by the same loop.
- Records outputs, candidate spends (every input whose ring holds one of the
  account's RingCT outputs), and the transactions they came in. Keeps 200
  block hashes and rewinds on a reorg to the fork.
- Looks at the mempool once caught up and reports pool receipts over
  `/get_address_txs` with `mempool: true`.
- Serves every call in the table above, plus `/daemon_status`. Picks ring
  members with wallet2's gamma. Packs the legacy 96-byte `rct` blob so
  MyMonero-style clients can decrypt it.
- Admin REST API as monero-lws defines it (`{"auth", "params"}`): accounts,
  requests, status, rescan, validate. Webhooks answer 501 for now.
- Policies: `--create off|auto|approve`, `--import auto|approve`,
  `--max-subaddresses`, `--lookahead`.
- Storage: one SQLite file (WAL). No LMDB, no MySQL.

Tried on 2026-11-07 against `stagenet.xmr-tw.org:38081`: a fresh account
logged in and was scanned to the tip; a rescan of 3,000 blocks finished in
about 15 s; `/get_random_outs` answered 15 unlocked members; a wrong view
key got 403; the admin API listed the account. No stagenet coins were sent
to the test wallet in that session (the faucet call was left for a person),
so receipt detection on the server is covered by monero-oxide's scanner and
the unit tests, not yet by a live payment.

Not built: webhooks and the `/feed` websocket, fiat `rates`, pre-RingCT
spends (ignored), import fees, remote scanning, the `get_tree_path` FCMP++
endpoint, TLS on the listener (put it behind a reverse proxy, as monero-lws
suggests too). The first version scans every block once per account with
monero-oxide alone, so its cost is accounts × blocks: fine for a shop or a
family, not for hundreds of wallets. The next section is how it scales.

## Scanning at scale: the prefilter on the server

Fob already has the right engine. The prefilter (`monokulo-8ar`, lifted out
of `fob-scanner` by decision 366; decisions 331, 332, 350, 351) doesn't
scan; it *prefilters*: for each transaction key `R` in a batch
of blocks it computes `8·a·R` once on the GPU (Vulkan, batches of 32,768,
the point compressed on the GPU too), and for each output checks the view
tag (`Keccak("view_tag" ‖ D ‖ i)[0]`, 1 in 256 pass) and then the real
thing, `P − H_s(D ‖ i)·G ∈ {our spend keys}`, with curve25519-dalek on the
CPU. Only blocks that hold a real receipt go to monero-oxide's `Scanner`.
It is GPU-only (decision 367): where there is no usable Vulkan GPU,
`flag` answers `None` and the caller scans every block with monero-oxide,
which is the fallback everywhere, Fob included. There is no size below
which a batch is kept off the GPU (decision 370): every batch goes to it
when it is there. Any Vulkan GPU does,
integrated ones included; Apple gets it through MoltenVK when there is an
Apple client. On an RX 6800 XT it put 5,000 mainnet blocks through in
0.29 s for one wallet, with 39 MB of GPU state however large the batch.

Today the kernel takes **one view key per dispatch**: the key's 64 signed
base-16 digits travel as push constants, and every work item multiplies a
different `R` by the same `a`. A server has the opposite shape: few new
`R`s a block (about 57), many `a`s. The change is small and lives in
`monokulo-8ar`, so Fob gets it too:

- **A keyring, not a key.** `Prefilter` becomes `Keyring`: `add(pair, view)
  -> AccountId`, `watch(id, pair, subaddress)`, and `flag_many(blocks,
  threads, gpu_allowed) -> Vec<Hit { account, block, tx }>`. The single-key
  `Prefilter::flag` stays as a keyring of one, so Fob's call site doesn't
  change.
- **Digits per item.** The shader gains a storage buffer of account digits
  (64 bytes an account; 1,000 accounts are 64 KB) and two push constants,
  `tx_count` and `first_item`, so work item `i` takes key `R[(first_item +
  i) mod tx_count]` and the digits of account `(first_item + i) div
  tx_count`. The grid of accounts × keys is never materialised: the input
  buffer still holds one copy of each `R`, and a batch of 32,768 items is a
  slab of the grid. State per item (1,200 B) and output per item (100 B) are
  as today, so memory doesn't grow with accounts. The stages (INIT, 16
  WINDOW passes, FINAL ×8) and the constant-time table lookups are
  untouched; the CPU-side pipelining (finish batch *n* while the GPU does
  *n*+1) is untouched.
- **Finish per account.** `finish` and `pays_us` already take a derivation
  and a spend set; they now take the item's account's spend set. Hashing is
  the CPU's share: accounts × outputs (about 135 a block) Keccaks, spread
  over cores. A later step moves compression and the view-tag Keccak onto
  the GPU too and returns only tag hits, leaving the CPU one scalar
  multiplication per hit (one in 256 outputs per account). That is phase B;
  phase A is already a different class of server.
- **One device, an arena per user of it.** The engine's `OnceLock<Mutex<Gpu>>`
  holds the device *and* its one arena (the 39 MB state buffer, output
  buffer, command buffer, fence) and keeps the lock for a whole batch. It
  splits into a shared `Device` (instance, logical device, queues, pipeline
  and cache, self-test) and per-caller `Arena`s of about 43 MB, the lock
  shrinking to the `vkQueueSubmit` Vulkan requires per queue, or to nothing
  where the GPU has several compute queues. Fob keeps one device and one
  arena, unchanged. `warm_up`, the self-test vectors, the 20–30 s shader
  compile with its pipeline cache and the retire-on-failure path all carry
  over, and there is nothing to choose: `monokulo-lws serve --cache-dir …`
  uses the GPU when it is usable and says so in `/daemon_status`. No GPU
  means the first version's loop, one monero-oxide scan per account per
  block: fine for a shop, not for hundreds of wallets, which want a machine
  with a GPU, integrated included.

What changes in `monokulo-lws`'s loop (`scanner.rs`):

1. Fetch 100 blocks (as now), keeping each transaction's first output index
   from `get_blocks.bin`, not only each block's.
2. Build (and cache, until an account or a subaddress range changes) the
   keyring from every active account.
3. `flag_many` → hits. A hit is one account and one transaction.
4. For each hit, monero-oxide's `Scanner` for that account alone, on a
   stand-in block holding that one transaction with its own first output
   index, so global indices stay right; its `WalletOutput`s become rows as
   today. Hits are rare, so this is near free.
5. Candidate spends stop being per account too: one map `global index →
   (account, output id)` over every account, one lookup per ring member,
   extended as outputs are found.
6. The pool is a stand-in block through the same keyring.
7. Fallbacks: a block `fob-scanner` won't vouch for (v1 transactions, no
   RingCT outputs, a hard fork newer than it knows) is scanned the old way
   for every account; outputs without a view tag (before August 2022) skip
   the tag and take the full derivation, slower but correct.

**A wallet joining with history** (a restore, an import, a rescan) gets a
lane of its own rather than dragging everyone back: the *live lane* is the
keyring of every caught-up account and follows the tip; a *catch-up lane*
fetches from the newcomer's start height with a keyring of only the
accounts behind, takes in others as its cursor passes their start heights,
and merges into the live keyring when it reaches the tip. One daemon
connection (live first), one GPU device with an arena per lane (about 43 MB
each, so lanes never wait on each other's batch), nothing new in the
database: an account's `scan_height` says which lane it is in. Fob sees
`scanned_height` below `blockchain_height` until the merge, hence the
"Server scanning from 2019" chip. Lanes are configurable
(`--catch-up-lanes`, `--catch-up-join follow|wait`, `--catch-up-blocks`,
`--catch-up-in-flight`, `--gpu-arenas`; `monokulo-lws/docs/scanning.md`
has the table); `--catch-up-lanes 0` is the first version's single loop,
fine for a handful of wallets.

What it buys, from decision 350's figure of about a million derivations a
second on a desktop GPU, to be measured:

| | work | time |
|---|---|---|
| 1,000 accounts, a day of mainnet (720 blocks × 57 keys) | 41 M derivations | about 40 s |
| one new account, a year of mainnet | 15 M | about 15 s |
| 100 accounts imported from 2022 | 150 M | a few minutes |
| GPU memory | 39 MB a batch | whatever the account count |

Shape of the dependency: the prefilter is its own crate and repository,
`monokulo-8ar` (decision 366), which both Fob's `fob-scanner` and
`monokulo-lws` depend on, pinned by revision until it is on crates.io (as
Fob pins `orf-codec`, decision 4). Fob's `fob-scanner` keeps only
`scan.rs` (the reader threads and the monero-oxide fan-out). The keyring
is a feature Fob could use itself one day (several wallets on one phone,
one pass).

Order of work: (a) the keyring API and the per-item digits buffer in
`monokulo-8ar`, with the existing vectors run per account and a test that a
keyring of *n* copies of one key flags what the single key flags; (b)
`monokulo-lws` on `flag_many`, the per-transaction stand-in blocks and the
global spend map, measured against the 2,000-block mainnet fixture with 1,
10, 100 and 1,000 keys; (c) view-tag Keccak on the GPU.
