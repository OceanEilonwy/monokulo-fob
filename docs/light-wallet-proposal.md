# Proposal: a Light Wallet mode for Fob

Status: proposal, 2026-11-07. Nothing in Fob is built yet; the server half
(`monokulo-lws`) exists as a first working version, in its own repository.

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
| `/get_address_txs` | Every transaction that touched the wallet: hash, height, time, `total_received`, `total_sent`, `unlock_time`, `coinbase`, `mixin`, `payment_id`, and per transaction the **candidate spends** (`spent_outputs`: amount, key image, the spent output's tx pub key and index, ring size) and, from monero-lws on, the **outputs received** (`receives`: amount, one-time key, index, global index, tx pub key, RingCT mask, subaddress). Plus `scanned_height`, `blockchain_height`. |
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
  in light mode, so this is cheap and idempotent.
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
  its rule (decision 48): outputs locked to a block are kept with that
  block, outputs locked to a time are dropped.
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
  the server sent. A server can hide outputs; it cannot invent amounts that
  pass those checks. The format is "not protocol defined" (monero-oxide's
  words), so this needs a round-trip test pinned to `monero-wallet 0.2`, and
  a small upstream PR for a `WalletOutput::new` would be the lasting fix.
- **Spent.** For each candidate in `spent_outputs`, find the owned output by
  (`tx_pub_key`, `out_index`), compute its key image (`keys.rs::key_image`,
  as `take_block` does today), and if it equals the candidate's, mark the
  output `spent_in` that transaction and push a `ChainSpend` (fee from the
  transaction). A view-only wallet can't, and shows what it shows today.
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

`prepare_payments` keeps its shape: pick coins largest first, build a
`SignableTransaction`, retry on `NotEnoughFunds`. What changes is where the
inputs' decoys, the fee and the tip come from:

- **Fee.** `/get_unspent_outs` gives `per_byte_fee` and `fee_mask` →
  `FeeRate::new(per_byte_fee, fee_mask)`, capped by `MAX_FEE_PER_WEIGHT` as
  today (decision 12), so a hostile server can't take a large fee.
- **Decoys.** One `/get_random_outs { count: 15, amounts: ["0"] × inputs }`.
  monero-oxide's `OutputWithDecoys::new` picks its own decoys from the
  distribution, which the API lacks; but `OutputWithDecoys::read` is public
  and its format is `OutputData ‖ Decoys` (`key ‖ key offset ‖ mask ‖ amount`
  then `monero_clsag::Decoys`, whose `Decoys::new(offsets, signer_index,
  ring)` is public). So Fob builds each input from the server's 15 members
  plus its own output: sort the 16 by global index, take the real one's
  position as `signer_index`, offsets as differences, ring as `[key,
  commitment]` pairs. Checks: 15 distinct members, none of them the wallet's
  own outputs, all older than 10 blocks per the server's own heights. A
  server that chooses decoys badly weakens the ring's privacy; it already
  knows which output is ours, so this is no new leak, and it is the trade
  every light wallet makes.
- **Tip.** `blockchain_height` from the last poll.
- **Submit.** `/submit_raw_tx { tx: hex(tx.serialize()) }`. `not_relayed`
  or a 422 become `publish_failure` words. The payment is recorded as sent
  at once, as today (decision 14), and the next poll shows it in the pool.

The decoy cache file (`.decoys`) isn't used or updated in light mode.

## The settings screen and the words

Settings › Node, with light mode on, reads "Light wallet server ·
lws.example.com". The screen:

- **Sync through**: Node | Light wallet server (one `Segmented`).
- **Server**: the URL (`https://host:port`, or `http://` for a server on the
  phone's own network, same rules as a node: no `.onion`, decision 73), a
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
| Hide a payment or a spend. | Not detectable; also the user's choice of server. Fob can at least show when the server's `scanned_height` lags the chain. |
| Invent an output. | Caught: the one-time key and the commitment are checked against the view key and the amount. |
| Report a wrong fee. | Capped by `MAX_FEE_PER_WEIGHT`. |
| Pick decoys that weaken a ring. | Checked for count, distinctness, age and not being ours; beyond that, the trade of the mode. |
| Not broadcast a transaction. | `not_relayed` and failures shown; the payment is still recorded, and the next poll shows it absent from the pool. |
| Learn the phone's address and when it's on. | Said in the words. Tor is still future work (decision 73) and would cover this. |

Transport: TLS as for nodes, including a pinned self-signed certificate. The
view key is in every request body, so plain `http://` should be allowed
only for a server on the local network, with the same warning a plain-http
node gets.

## Phases

1. **Server** (done, first version): `monokulo-lws`, below.
2. **Fob, read-only light mode**: `fob-chain::light`, the `Backend` setting
   and schema migration, `light_step` with `reconcile`, the Node screen and
   words, the chip. Sending in light mode disabled with "Sending through a
   server comes next". Tested against `monokulo-lws` on stagenet, and
   against monero-lws in Docker for compatibility.
3. **Sending**: `SendSource` for the light client, inputs from server
   decoys, submit. Tested end to end on stagenet.
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
suggests too). Cost is linear in accounts × blocks, which is fine for a
shop or a family and not for a public service; sharing the per-transaction
`8·a·R` work across accounts isn't possible (each account has its own `a`),
so a large server would want the view-tag prefilter per account on a GPU, as
`fob-scanner` does.
