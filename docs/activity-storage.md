# Proposal: activity built once, receipts read once

Status: built on branch `activity-in-sqlite` (decisions 411–414), for
finding 19 of the round-2 review: Home and Contacts rebuilt every payment
row, parsed every receipt and copied every contact's photo about once a
second while a wallet caught up.

## What it is

`Wallet::activity()` turns what the wallet holds (its outputs, chain spends,
payments sent, the pool, requests, contacts, notes, shares and price records)
into one row per transaction. Home, Contacts, a request's screen, a contact's
card, Your shares and Send's sheet are all built from those rows, and
`Wallet::screens()` asks for them after every sync report (`REPORT_EVERY`,
1 s, while catching up).

Before this change every call:

- parsed each sent payment's ORF receipt JSON with serde and built its
  `ReceiptView` again;
- for each payment out, ran through every output to find those it spent,
  hex-decoding the payment's hash once per output (`Owned::spent_by`), and
  again to find its change;
- for each payment in, ran through every output for its transaction, every
  request for its address (`requests::Book::at`) and every share;
- for each payment out, ran through every contact's addresses
  (`contacts::Book::owner`);
- for each pool entry, ran through every row built so far;
- and handed back a fresh `Vec<Activity>`, each row a deep copy, receipts
  and their lines included.

That is O(rows × outputs) plus O(rows × contacts) plus a JSON parse per
receipt, under the state lock, every second. A restored wallet with a few
thousand payments spent tens of milliseconds on a desktop doing it, and the
sync step's apply waited on the lock meanwhile. Every contact row then
carried its photo (up to 8 KB) across the FFI on every redraw.

## What changes

### 1. A receipt is read once, when the payment is recorded (SQLite)

`Sent` gains `receipt_view: Option<Arc<ReceiptView>>`, kept in a new
`sent.receipt_view` column (JSON). It's made by `ReceiptView::from` the
moment a payment is recorded (`record_sent`, which every way of sending
ends in), with the receipt's text cleaned as it is today, and never again.
A receipt is
the payee's words about a payment that has happened: it never changes, so
the row diff in `Db::save` writes the column once and never again.

Activity, the coins' sources ("Change from Corner Café") and anything else
that needs the merchant, the lines, the total or the item count read the
stored view. The raw JSON stays in `sent.receipt` as the record of what the
code said.

Database version 11 adds the column and fills it for every payment already
recorded, in the upgrade's one transaction (see *Upgrading*).

### 2. Activity is built once per change, with indexes (memory)

`Wallet::activity()` returns `Arc<[Activity]>`, cached in the wallet with
the key it was built for:

- **the state's generation.** The state lives in a `Tracked<State>`: any
  mutable borrow of it (through `DerefMut`, the only way to change it) takes
  a new number from a process-wide counter. Nothing has to remember to say
  it changed something, so nothing can forget. A clone keeps its number
  and its content together; putting a changed clone back (`*state =
  changed`) brings the clone's new number with it.
- **the pool's generation**, the same way (`Tracked<HashMap<_, Pooled>>`).

An unchanged key returns the cached `Arc`: no work at all, not even a copy.
A changed key rebuilds the rows in one pass with maps built once:

| question | before | now |
| --- | --- | --- |
| outputs a payment out spent | scan all outputs, hex-decode per output | `HashMap<[u8; 32], Vec<&Owned>>` from each coin's `tx()` |
| a transaction's own outputs | scan all outputs | `HashMap<&str, Vec<&Owned>>` |
| a payment out's record | scan `sent` | `HashMap<&str, &Sent>` |
| the request on an address | scan requests | `HashMap<u32, &Request>` |
| the share on an address | scan shares | `HashMap<u32, &Share>` |
| the contact owning an address | scan contacts × addresses | `HashMap<&str, &Contact>` |
| a pool entry already a row | scan all rows | `HashSet<&str>` |
| the receipt | serde parse + view per call | the stored `Arc<ReceiptView>` |

That is O(rows + outputs). `Activity.receipt` becomes `Option<Arc<ReceiptView>>`,
so copying a row copies a pointer, not the receipt's lines, and
`Activity.receipt_json`, which nothing read, goes.

The screens' lists (`Wallet::lists`) are cached the same way, keyed by the
activity's key and the clock's calendar day, UTC offset and 12/24 h
setting, the only parts of the clock the lists use. A refresh with nothing
new rebuilds nothing.

The tip, the last block's time and the Manual Sync settings, which
confirmations and `needed` depend on, are part of the key too. While
catching up they change with every batch, so the rows are rebuilt each
second then: but that's the one-pass build above, a few milliseconds for
5,000 rows (see *Measured*), not the quadratic one.

### 3. Photos cross the FFI once (FFI, Kotlin, GTK)

`contacts::Row` carries `photo_key: Option<String>` instead of the photo:
the contact's id and a hash of the photo's bytes (`"7-1f3a…"`), worked out
when the row is made. `Wallet::contact_photo(id)` hands over the bytes. The
Android `Avatar` and GTK's `widgets::contact_avatar` keep the decoded image
by key; a new key (the photo was edited) misses the cache and fetches once.
The bytes and the decoding happen once per photo per run, not once per row
per redraw.

## What stays in memory, and why

The proposal behind this finding asked for activity rows materialised in
SQLite, with indexes, updated incrementally, and for the screens to ask SQL.
They aren't, deliberately:

- **The wallet's state in memory is the truth, and SQLite lags it.** While
  catching up, scanning is saved every 30 s (`SAVE_EVERY`) unless money
  moved; the pool is never saved (it's asked again after every block).
  Rows read from SQLite would have to be merged with the pool and with
  unsaved scanning in Rust anyway.
- **Home needs every row.** Its Activity list is the whole history, so the
  rows are in memory for each refresh whatever SQLite holds. Per-address
  and per-request totals over rows already in memory are one pass with a
  map; asking SQLCipher for them decrypts pages to answer what memory
  already knows.
- **Writes cost on a phone.** Every SQLCipher write encrypts pages and,
  with `synchronous = FULL`, waits for the flash. Rows derived from the
  outputs and spends would be written again with every change to those,
  doubling what a save writes, for data that one pass rebuilds in
  milliseconds from what's already loaded.
- **Derived rows can go stale.** A stored row is only right while the code
  that derived it is the code reading it; every change to how activity is
  worked out would need a schema version to rebuild the table. A receipt's
  view is the exception that proves it: it's derived only from the receipt,
  which never changes, and `ReceiptView::from` changing would only change
  how old receipts look, which a later upgrade can redo.

So SQLite keeps what's immutable and costly to derive (the receipt's view),
and memory keeps what's cheap to derive and changes with the chain. The
row-level incremental update is the cache key instead: a refresh with
nothing new is no work, and anything new is one linear pass.

## Upgrading (version 10 → 11)

In one transaction, as version 9 → 10 was:

1. `ALTER TABLE sent ADD COLUMN receipt_view TEXT`.
2. For every `sent` row with a `receipt`, its view's JSON, written into
   `receipt_view` (fob-vault doesn't read ORF, so `Db::open` is handed the
   function that does).
3. `PRAGMA user_version = 11`.

A crash before the commit leaves version 10 whole, and the next open does
it again; a crash after leaves version 11. Opening a version 11 file is no
upgrade. A payment whose receipt doesn't read gets no view, as Activity
showed none for it before. Opening the sealed file from before the
database, which may be from any version, fills missing views as it loads
(`State::fill_receipt_views`), which also covers any payment recorded
without one.

There's no way back to version 10: a version 10 Fob refuses a version 11
file, as with every version before. The column is added, nothing is
removed, so the data a version 10 Fob would need is all still there.

## Measured

`cargo test --release -p fob-wallet --lib activity_over_a_long_history -- --ignored --nocapture`
builds a wallet with about 5,000 payments (3,000 in, some to requests,
some in two pieces; 1,500 out, a third with a receipt, most to contacts,
every tenth a split; 500 spends a restore found), 200 contacts with photos
and 100 requests, and times each call (release build, a desktop, best of
20; 5,005 rows):

| call | before | after |
| --- | --- | --- |
| `activity()`, nothing changed | 2.63 s | 50 ns (the kept `Arc`) |
| `activity()` built afresh | 2.63 s | 3.7 ms |
| `lists()`, nothing changed | 2.74 s | 100 ns |
| `lists()` after a block (the tip moved) | 2.66 s | 7.9 ms |
| `contacts("")` | 2.62 s | 0.37 ms |

The quadratic searches were nearly all of the old time: 1,500 payments
out, each looking through 4,600 outputs and hex-decoding its hash for
every one. A phone is slower than this desktop by perhaps five to ten
times; before, that was tens of seconds a refresh on this history, now
tens of milliseconds while a wallet catches up and nothing once it has.

What's left on the catching-up path is the rebuild itself (string copies
for 5,000 rows, the maps) and the lists' words. Taking the tip out of the
key, so a block only refreshes confirmations, would save the maps but not
the copies, as each row carries its confirmations; it isn't worth a second
generation to keep right.

## Proving it's the same

`tests/activity.rs` keeps the old algorithm, word for word apart from
taking the state as an argument, as the oracle, and compares its rows with
the new ones over a fixture with every kind of row: received in one and
several pieces, to requests (live, deleted before and after paying),
shares and noted addresses; sent with and without receipts, to contacts,
splits and to this wallet's own address (mined and in the pool); restored
spends with and without a fee; payments not confirmed and released; pool
payments; after a rewind; and a light mode wallet. Under `cfg(test)`,
every `activity()` call also builds the rows afresh and asserts the cached
ones are the same, so every existing test checks the cache too.
