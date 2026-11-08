# Proposal: a coin's state as a type, not a string

Status: built and released in Fob 0.1.16 (PR #29, merged 2026-11-08;
decision 400, which also records what was changed from this document in the
building).
Prompted by decision 399: a Trezor wallet's balance fell when its key image
sync found spent coins and climbed back eight minutes later, because one of
eight places that write a coin's spent mark didn't know what the mark
meant. The transition table lives in the module comment of
`crates/fob-wallet/src/wallet/coin.rs`, so the code and the account of it
are one file; what follows is the proposal as it was made.

## What it is

Every coin the wallet owns (`Owned`, in `crates/fob-wallet/src/wallet/state.rs`)
carries its spent state in one field, `spent_in: Option<String>`, that means
six different things:

| `spent_in` | meaning | decided by |
| --- | --- | --- |
| `None` | free, or a device's coin not yet asked about | `key_image` being `Some` |
| hash in `state.spends` | spent on the chain, transaction known | a lookup |
| hash in `state.sent` | held by a payment of ours, mined or not | a lookup |
| hash in neither, in the node's pool | held by a pool transaction | `scan_pool` |
| `"spent-before-sync"` | a device sync: the node said spent, transaction unknown | `hardware::is_sync_mark` |
| `"spent-in-pool-at-sync"` | a device sync: in the pool only, tentative | `hardware::is_sync_mark` |

Eight places write it: the block scan (`sync.rs`, a key image seen spent),
the pool poll (`pool_spends`, `pool_left`), the reorganisation rewind, the
light server reconcile (three sites in `light.rs`), the device sync
(`hardware::mark_spent_by_node`), sending (`send.rs`) and releasing a
payment (`manual_sync::release_payment`). Each decides on its own which
of the six it is looking at and which it may turn it into. Decision 364
taught two of them about the sync marks; decision 399 found the third.
The next one will be found the same way: by someone watching the balance.

The proposal: one pure module, `wallet/coin.rs`, with the state as an enum,
every change as an event, and one exhaustive function from (state, event)
to state. The compiler then refuses a forgotten pair. The eight writers
become eight emitters of events; a coin is spendable only through a type
that can be made from exactly one state.

## Why

- **Correctness the compiler checks.** `pool_left`'s mistake was a missing
  case. With `match (coin, event)` there is no such thing as a missing
  case: every pair is written down or the crate doesn't build.
- **One place to read.** What can happen to a coin is one function, not a
  grep across five files. Decision 364's rules (marks survive a rewind,
  every sync asks again, pool spends are tentative) become arms with the
  decision number on them.
- **A log of why.** Every transition goes through one door, so it can be
  logged with the coin's id. The eight-minute mystery would have been one
  line: `coin a1b2… Held(DeviceChain) --PoolLeft--> Free`.
- **The balance stops lying for device wallets.** Today a Trezor coin whose
  key image the device hasn't given is counted as unlocked, though it may
  have been spent long ago; the Home banner is the only warning. In the
  new model it is a state of its own, and the balance says so.
- **Tests that mean something.** Random sequences of events against
  invariants ("Spent never becomes Free except by a rewind below its
  height") catch the class, not the instance.

## The types

```rust
/// A coin's key image: a device's coin has none until the device is asked.
pub enum KeyImage { Unknown, Known([u8; 32]) }

/// Where a coin stands. Confirmations are not part of it: they are a
/// function of its height and the tip, decided when the question is asked.
pub enum Coin {
    /// Spendable once confirmed, if its key image is known.
    Free { ki: KeyImage },
    /// Not spendable now; the hold can be lifted.
    Held { ki: KeyImage, hold: Hold },
    /// Spent on the chain. Undone only by a rewind below `height`.
    Spent { ki: KeyImage, tx: TxHash, height: u64 },
}

pub enum Hold {
    /// A transaction in the node's or the server's pool spends it.
    InPool { tx: TxHash },
    /// A payment of ours spends it; lifted by release_payment, or when the
    /// payment is seen mined (then Spent).
    OurPayment { tx: TxHash },
    /// A device's key image sync: the node said spent on the chain, and no
    /// transaction is named. Lifted only by the next sync (decision 364).
    DeviceChain { asked: u64 },
    /// A device's key image sync: in the pool only. Tentative: the next
    /// sync decides, or the pool transaction, once named, leaving.
    DevicePool { asked: u64 },
}

/// What an observer saw. Each is a fact about the world, not a decision.
pub enum Event {
    /// The device gave the coin's key image.
    LearnedKeyImage([u8; 32]),
    /// A block scanned, or the server, shows this transaction spending it.
    MinedSpend { tx: TxHash, height: u64 },
    /// The node's pool, or the server's, has this transaction spending it.
    PoolSpend { tx: TxHash },
    /// This transaction is no longer in the pool and wasn't seen mined.
    PoolLeft { tx: TxHash },
    /// The node's answer to is_key_image_spent at a device sync.
    DeviceSaid { answer: Said, at: u64 },
    /// We built and broadcast a payment spending it.
    SentByUs { tx: TxHash },
    /// The person released that payment.
    Released { tx: TxHash },
    /// Blocks from `from` on are forgotten: a reorganisation or a rescan.
    Rewind { from: u64 },
}

pub enum Said { Unspent, OnChain, InPool }

/// The one function. Exhaustive over (Coin, Event).
pub fn step(coin: Coin, event: &Event) -> Coin;
```

`TxHash` is `[u8; 32]`, never a `String`: the sentinel words can't be
written by accident because they aren't hashes.

### The table

The table of states against events, each cell the state after, is the
module comment of `coin.rs`, beside the arms of `step` that are its cells
in its order. It isn't repeated here, so there is one copy to keep true.

Notes on the cells that carry a decision:

- `MinedSpend` wins over everything but a later rewind: the chain is the
  truth. A `Held OurPayment` seen mined becomes `Spent` and the payment's
  `height` is set by the same observation, as today.
- `PoolLeft` frees only a hold that names that transaction. The device
  holds name none, so they stay (decision 364; the bug of 399).
- `PoolSpend` on `Held DevicePool` names the transaction (decision 399),
  so the pool's leaving can free it.
- `DeviceSaid Unspent` frees a device hold: the node was asked again and
  says no (decision 364). It does not free `InPool` or `OurPayment`: those
  name a transaction the device sync didn't look at.
- `Rewind` applies to the *coin list*, not just a coin: a coin whose own
  height is at or above `from` is dropped from the list. `step` sees
  `Rewind` only for coins that stay, and for those it only undoes a `Spent`
  whose spending height is at or above `from`. A device's coin dropped by a
  rewind is remembered by id (see Rewind below) so its key image and device
  hold come back with it.
- `LearnedKeyImage` changes `ki` in any state and nothing else.

## Where it goes in the crates

- `crates/fob-wallet/src/wallet/coin.rs`: the types above, `step`, and
  `Coin::spendable(&self, confirmations, needed) -> Option<Spendable>`.
  No IO, no `State`, no `Wallet`. Its tests are the table.
- `Owned` gains `coin: Coin` and loses `spent_in` and `key_image` (the key
  image lives in `Coin`). The hex `String` fields that fed them go with
  them; `key_image_hex()` stays as a view for the device sync's requests.
- `State` gains `apply(&mut self, id: &CoinId, event: Event) -> bool`
  (whether anything changed), the one door, which also pushes to a
  bounded in-memory `transitions: VecDeque<(CoinId, Coin, Event, Coin)>`
  for the log and the sync card's detail.
- `Spendable` is a newtype in `coin.rs` with a private field; `send.rs`'s
  `prepare` takes `Vec<Spendable>` from `State::spendable_coins()`. A
  device wallet's `Free { ki: Unknown }` coins can't become one, and are
  counted by `trezor_waiting` as today.
- `funds.rs`: `balance` and `coin_pieces` match on `Coin`. A new
  `Balance.unsynced` carries the device coins the wallet can't vouch for,
  so the screens can say "x XMR not yet checked with the Trezor" instead
  of counting it as unlocked.

### The emitters

| today | emits |
| --- | --- |
| `sync.rs` block scan, key image seen in an input | `MinedSpend` |
| `sync.rs` `pool_spends` | `PoolSpend` |
| `sync.rs` `pool_left` | `PoolLeft` per transaction gone |
| `sync.rs` `rewind` | drops coins by height, `Rewind { from }` to the rest |
| `light.rs` reconcile, mined candidate spend | `MinedSpend` |
| `light.rs` reconcile, device coin in a pool spend | `PoolSpend` / `PoolLeft` (the server's pool, by transaction) |
| `light.rs` reconcile, "gone" | drops coins the server no longer vouches for, as today |
| `hardware.rs` `trezor_sync` | `LearnedKeyImage` per coin, then `DeviceSaid` per coin from `is_key_image_spent` |
| `send.rs` `publish_and_record` | `SentByUs` |
| `manual_sync.rs` `release_payment` | `Released` |

`pool_spends`, `pool_left`, `is_sync_mark`, `SPENT_BEFORE_SYNC` and
`SPENT_IN_POOL_AT_SYNC` are deleted. `hardware::is_sync_mark` has no
callers left.

### Rewind, and a device's coins

`rewind(from)` today drops every coin at or above `from` and the scan finds
them again as new, with no key image and no hold. For a device wallet that
means a reorganised stretch counts coins the device knows are spent until
the next sync (noted in decision 399). `State` keeps
`remembered: HashMap<CoinId, (KeyImage, Option<Hold>)>` for device coins a
rewind dropped, bounded to the dropped set; when the scan finds a coin whose
id is remembered, it starts from the remembered state instead of
`Free { Unknown }`. The entry is removed on use, and the whole map on a
device sync (which decides everything afresh).

## Persistence

`Owned` is saved as a row of the `outputs` table (`fob-vault`'s `Table`,
`user_version` 9 today). Version 10:

- adds `coin TEXT`, the `Coin` as JSON (`serde`'s externally tagged enum,
  hashes as hex), and keeps `key_image` and `spent_in` as nullable columns
  that are written as `NULL` from then on;
- the upgrade fills `coin` from the two old columns by the table in What
  it is, in SQL where it can (`NULL` → `Free`, the two words → the device
  holds with `asked = 0`) and in Rust for the hashes, which need
  `spends`, `sent` and the pool to classify: a hash in `spends` → `Spent`
  at that spend's height; in `sent` → `Held OurPayment`; anything else →
  `Held InPool` (the next pool poll frees it if the pool doesn't have it,
  as today);
- `remembered` is in-memory only: a rewind and the re-finding happen within
  one run, and a restart's scan asks the device again anyway.

Opening a v9 file on a build that writes v10 is one-way, as every upgrade
here is; the sealed `.fob` beside it is unchanged.

## Tests

- `coin.rs`: one test per row of the table, each event against it.
- Property tests (`proptest`, already a dev-dependency of `fob-wallet` or
  added as one): random sequences of events from a random start, asserting
  - `Spent` never becomes `Free` except by `Rewind { from }` with
    `from <= height`;
  - a `DeviceChain` or `DevicePool` hold is never lifted by `PoolLeft`;
  - `Spendable` is never made from `ki: Unknown`;
  - `LearnedKeyImage` never changes anything but `ki`;
  - `step` is total: no panic for any pair.
- The regression of 399 as a named sequence: `Free` → `DeviceSaid OnChain`
  → `PoolLeft(other)` → still held.
- The existing `hardware`, `sync` and `light` tests keep passing, with their
  assertions on `spent_in` rewritten against `Coin`.

## Phases

1. `coin.rs` with the types, `step`, `Spendable` and all tests. No caller.
2. `Owned.coin` beside the old fields, written in step with them; `balance`
   and `prepare` read from `coin`. Both agree in every test (a debug
   assertion compares them for a release).
3. The emitters, one file at a time, each deleting its writes to `spent_in`.
   `pool_spends`/`pool_left` go last, with decision 399's test moved to
   `coin.rs`.
4. Schema 10, the old columns nulled, the sentinels deleted, `Balance.unsynced`
   on both screens, the rewind memory.

Each phase is a commit that builds and passes. Decision 400 records the
model; the table in this document moves into `coin.rs`'s module comment, so
the code and the account of it are one file.

## Decisions to make

- Whether `Balance.unsynced` shows on Home as its own line or stays inside
  the Trezor banner's words. Proposed: the banner, with the amount in it.
- Whether the transition log is kept across runs (a table) or in memory.
  Proposed: memory, 1,000 entries, shown under the sync card's detail;
  a file can come later if a mystery survives a restart.
- Whether `OurPayment` should be lifted by `PoolLeft` of its own hash after
  a day, as `check_quiet_payments` marks it not confirmed. Proposed: no;
  release stays the person's act (decision 364's "the coins stay spent
  until someone releases them").
