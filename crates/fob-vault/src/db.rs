//! A wallet's database: one SQLCipher file per wallet, under the 32-byte key
//! the app keeps behind the Keystore, used as a raw key (it's random already,
//! so no slow key stretching on opening). Every page is encrypted, indexes and
//! write-ahead log too, as the sealed file before it was.
//!
//! The wallet still works on its [`State`](crate::wallet) in memory and hands
//! it here whole to save. Each save compares every row with what's on disk and
//! writes only those that changed, in one transaction, so a save costs what
//! changed and a crash leaves the last one whole.
//!
//! The columns are mapped through serde's JSON form of the state, so the
//! `#[serde(default)]`s keep doing what they did. Saving refuses a field with
//! no column rather than dropping it, so a new field can't go unsaved quietly.
//! Small lists inside a row (`back`, `also_paid_at`, a history's `days`) and a
//! request's `asked` are JSON in a column: nothing asks for them apart.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, ErrorCode, TransactionBehavior, params_from_iter, types::Value as Sql};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::{error::VaultError, error::Result, keys::StoredKeys};

/// `PRAGMA user_version` for this layout.
const VERSION: i32 = 8;

fn failed(e: impl std::fmt::Display) -> VaultError {
    VaultError::Storage(e.to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// An unsigned whole number. SQLite's integers are signed, and it would turn
    /// a larger one given as text into an inexact float, so all 64 bits are kept
    /// as they are: one above `i64::MAX` reads as negative in plain SQL.
    Int,
    Text,
    /// 0 or 1; a missing one is false.
    Bool,
    /// A list or structure, as JSON text.
    Json,
}
use Kind::{Bool, Int, Json, Text};

struct Table {
    name: &'static str,
    columns: &'static [(&'static str, Kind)],
}

const WALLET: Table = Table {
    name: "wallet",
    columns: &[
        ("node", Text),
        ("node_self_signed", Bool),
        ("zmq_endpoint", Text),
        ("zmq_on", Bool),
        ("restore_height", Int),
        ("next_block", Int),
        ("tip", Int),
        ("last_block_time", Int),
        ("receive_index", Int),
        ("receive_own", Int),
        ("split_index", Int),
        ("restored", Bool),
        ("fresh_from", Int),
        ("backup_pending", Bool),
        ("price_currency", Text),
        ("price_provider", Text),
        ("price_transport", Text),
        ("price_fiat_first", Bool),
        ("contacts_next_id", Int),
        ("requests_next_id", Int),
        ("profile_next_id", Int),
        // Since version 3.
        ("gpu_checked_on", Text),
        ("gpu_check_passed", Bool),
        // Since version 4.
        ("manual_sync", Bool),
        ("pull_mode", Text),
        ("incoming_scan_blocks", Int),
        ("settled_confirmations", Int),
        ("pull_mark_height", Int),
        ("pull_mark_hash", Text),
        // Since version 6.
        ("node_pin", Text),
        // Since version 8.
        ("sync_mode", Text),
        ("light_url", Text),
        ("light_self_signed", Bool),
        ("light_pin", Text),
        ("light_registered", Bool),
        ("light_subaddrs_sent", Int),
        ("light_scanned_height", Int),
    ],
};
const RECENT: Table = Table { name: "recent_blocks", columns: &[("height", Int), ("hash", Text)] };
const OUTPUTS: Table = Table {
    name: "outputs",
    columns: &[
        ("output", Text),
        ("tx", Text),
        ("height", Int),
        ("time", Int),
        ("amount", Int),
        ("subaddress", Int),
        ("coinbase", Bool),
        ("unlock_block", Int),
        ("key", Text),
        ("key_image", Text),
        ("spent_in", Text),
        ("tx_key", Text),
        ("extra_key", Text),
    ],
};
const SPENDS: Table = Table { name: "spends", columns: &[("tx", Text), ("height", Int), ("time", Int), ("fee", Int)] };
const SENT: Table = Table {
    name: "sent",
    columns: &[
        ("tx", Text),
        ("to", Text),
        ("amount", Int),
        ("fee", Int),
        ("change", Int),
        ("time", Int),
        ("height", Int),
        ("receipt", Text),
        ("note", Text),
        ("pieces", Int),
        ("back", Json),
        ("recipient_name", Text),
        // Since version 5.
        ("not_confirmed", Bool),
        ("released", Bool),
        // Since version 6.
        ("signed", Text),
    ],
};
const NOTES: Table = Table { name: "notes", columns: &[("subaddress", Int), ("note", Text)] };
const PRICE_CURRENT: Table = Table { name: "price_current", columns: &[("currency", Text), ("provider", Text), ("price", Int), ("at", Int)] };
const PRICE_HISTORY: Table = Table { name: "price_history", columns: &[("currency", Text), ("provider", Text), ("fetched", Int), ("days", Json)] };
const PRICE_RECORDS: Table = Table {
    name: "price_records",
    columns: &[("tx", Text), ("currency", Text), ("price", Int), ("provider", Text), ("at", Int), ("daily", Bool)],
};
const CONTACTS: Table = Table {
    name: "contacts",
    columns: &[
        ("id", Int),
        ("name", Text),
        ("note", Text),
        ("photo", Text),
        ("send_address", Text),
        ("also_paid_at", Json),
        ("created", Int),
        ("half", Bool),
    ],
};
const REQUESTS: Table = Table {
    name: "requests",
    columns: &[
        ("id", Int),
        ("subaddress", Int),
        ("address", Text),
        ("contact", Int),
        ("note", Text),
        ("asked", Json),
        ("created", Int),
        ("deleted", Int),
        ("paid_before_delete", Json),
        ("name", Text),
    ],
};
const PROFILE: Table = Table { name: "profile", columns: &[("name", Text), ("photo", Text), ("name_in_requests", Bool)] };
const SHARES: Table = Table {
    name: "shares",
    columns: &[("id", Int), ("subaddress", Int), ("address", Text), ("kind", Json), ("created", Int)],
};

/// Nodes this wallet was given besides its network's own. Since version 2.
const CUSTOM_NODES: Table = Table { name: "custom_nodes", columns: &[("url", Text), ("self_signed", Bool)] };

const TABLES: [&Table; 14] = [&WALLET, &RECENT, &OUTPUTS, &SPENDS, &SENT, &NOTES, &PRICE_CURRENT, &PRICE_HISTORY, &PRICE_RECORDS, &CONTACTS, &REQUESTS, &PROFILE, &SHARES, &CUSTOM_NODES];

fn sql_type(kind: Kind) -> &'static str {
    match kind {
        Int | Bool => "INTEGER",
        Text | Json => "TEXT",
    }
}

fn encode(kind: Kind, value: &Value, column: &str) -> Result<Sql> {
    let wrong = || failed(format!("{column} isn't what its column holds: {}", value_kind(value)));
    Ok(match (kind, value) {
        (Bool, Value::Null) => Sql::Integer(0),
        (Bool, Value::Bool(b)) => Sql::Integer(i64::from(*b)),
        (_, Value::Null) => Sql::Null,
        (Int, Value::Number(n)) => Sql::Integer(n.as_u64().ok_or_else(wrong)? as i64),
        (Text, Value::String(s)) => Sql::Text(s.clone()),
        (Json, v) => Sql::Text(v.to_string()),
        _ => return Err(wrong()),
    })
}

/// The kind of a value, for an error: never the value, which may be private.
fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "a structure",
    }
}

fn decode(kind: Kind, value: Sql, column: &str) -> Result<Value> {
    let wrong = || failed(format!("{column} in the database isn't what its column holds"));
    Ok(match (kind, value) {
        (Bool, Sql::Integer(i)) => Value::Bool(i != 0),
        (_, Sql::Null) => Value::Null,
        (Int, Sql::Integer(i)) => json!(i as u64),
        (Text, Sql::Text(t)) => Value::String(t),
        (Json, Sql::Text(t)) => serde_json::from_str(&t).map_err(|_| wrong())?,
        _ => return Err(wrong()),
    })
}

/// Takes `key` out of `map`: nothing if it's not there.
fn take(map: &mut Map<String, Value>, key: &str) -> Value {
    map.remove(key).unwrap_or(Value::Null)
}

fn object(value: Value, what: &str) -> Result<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        other => Err(failed(format!("{what} should be a structure, not {}", value_kind(&other)))),
    }
}

fn list(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        _ => vec![],
    }
}

/// Refuses whatever's left of `map`: a field no table has a column for.
fn all_taken(map: &Map<String, Value>, what: &str) -> Result<()> {
    match map.keys().next() {
        Some(field) => Err(failed(format!("{what}.{field} has no column"))),
        None => Ok(()),
    }
}

/// A row of `table` from `row`, its columns in order, refusing any field left over.
fn encode_row(table: &Table, mut row: Map<String, Value>) -> Result<Vec<Sql>> {
    let encoded = table.columns.iter().map(|(column, kind)| encode(*kind, &take(&mut row, column), column)).collect::<Result<Vec<_>>>()?;
    all_taken(&row, table.name)?;
    Ok(encoded)
}

type Rows = Vec<(&'static Table, Vec<Vec<Sql>>)>;

/// The state's JSON form as rows, table by table.
fn to_rows(state: Value) -> Result<Rows> {
    let mut s = object(state, "state")?;
    let mut prices = object(take(&mut s, "prices"), "prices")?;
    let mut contacts = object(take(&mut s, "contacts"), "contacts")?;
    let mut requests = object(take(&mut s, "requests"), "requests")?;
    let mut profile = object(take(&mut s, "profile"), "profile")?;
    let mut cache = object(take(&mut s, "price_cache"), "price_cache")?;

    let rows_of = |table: &'static Table, items: Vec<Value>| -> Result<(&'static Table, Vec<Vec<Sql>>)> {
        let rows = items.into_iter().map(|item| encode_row(table, object(item, table.name)?)).collect::<Result<Vec<_>>>()?;
        Ok((table, rows))
    };

    let recent = list(take(&mut s, "recent")).into_iter().map(|pair| match pair {
        Value::Array(mut pair) if pair.len() == 2 => {
            let hash = pair.pop().expect("two");
            json!({"height": pair.pop().expect("two"), "hash": hash})
        }
        _ => Value::Null,
    });
    let recent = rows_of(&RECENT, recent.collect())?;
    let outputs = rows_of(&OUTPUTS, list(take(&mut s, "outputs")))?;
    let spends = rows_of(&SPENDS, list(take(&mut s, "spends")))?;
    let sent = rows_of(&SENT, list(take(&mut s, "sent")))?;
    let custom_nodes = rows_of(&CUSTOM_NODES, list(take(&mut s, "custom_nodes")))?;

    // In order, so an unchanged map is unchanged rows.
    let mut notes: Vec<(u64, Value)> = object(take(&mut s, "notes"), "notes")?
        .into_iter()
        .map(|(subaddress, note)| Ok((subaddress.parse::<u64>().map_err(failed)?, note)))
        .collect::<Result<_>>()?;
    notes.sort_by_key(|n| n.0);
    let notes = rows_of(&NOTES, notes.into_iter().map(|(subaddress, note)| json!({"subaddress": subaddress, "note": note})).collect())?;

    let mut records: Vec<(String, Value)> = object(take(&mut s, "price_records"), "price_records")?.into_iter().collect();
    records.sort_by(|a, b| a.0.cmp(&b.0));
    let mut record_rows = vec![];
    for (tx, kept) in records {
        for record in list(kept) {
            let mut record = object(record, "price_records")?;
            record.insert("tx".into(), Value::String(tx.clone()));
            record_rows.push(Value::Object(record));
        }
    }
    let records = rows_of(&PRICE_RECORDS, record_rows)?;

    let current = take(&mut cache, "current");
    let current = rows_of(&PRICE_CURRENT, if current.is_null() { vec![] } else { vec![current] })?;
    let history = rows_of(&PRICE_HISTORY, list(take(&mut cache, "history")))?;
    all_taken(&cache, "price_cache")?;

    let contact_rows = rows_of(&CONTACTS, list(take(&mut contacts, "contacts")))?;
    let request_rows = rows_of(&REQUESTS, list(take(&mut requests, "requests")))?;
    let you = take(&mut profile, "profile");
    let you = rows_of(&PROFILE, if you.is_null() { vec![] } else { vec![you] })?;
    let shares = rows_of(&SHARES, list(take(&mut profile, "shares")))?;

    let mut wallet = Map::new();
    for (column, value) in [
        ("price_currency", take(&mut prices, "currency")),
        ("price_provider", take(&mut prices, "provider")),
        ("price_transport", take(&mut prices, "transport")),
        ("price_fiat_first", take(&mut prices, "fiat_first")),
        ("contacts_next_id", take(&mut contacts, "next_id")),
        ("requests_next_id", take(&mut requests, "next_id")),
        ("profile_next_id", take(&mut profile, "next_id")),
    ] {
        wallet.insert(column.into(), value);
    }
    all_taken(&prices, "prices")?;
    all_taken(&contacts, "contacts")?;
    all_taken(&requests, "requests")?;
    all_taken(&profile, "profile")?;
    // What's left of the state is the wallet row's own.
    wallet.append(&mut s);
    let wallet = encode_row(&WALLET, wallet)?;

    Ok(vec![(&WALLET, vec![wallet]), recent, outputs, spends, sent, notes, current, history, records, contact_rows, request_rows, you, shares, custom_nodes])
}

/// Rows read back into the state's JSON form.
fn from_rows(mut tables: HashMap<&'static str, Vec<Map<String, Value>>>) -> Result<Value> {
    let mut take_table = |name: &str| tables.remove(name).unwrap_or_default();
    let mut w = take_table(WALLET.name).pop().ok_or_else(|| failed("the database has no wallet row"))?;
    let values = |rows: Vec<Map<String, Value>>| rows.into_iter().map(Value::Object).collect::<Vec<_>>();
    let recent: Vec<Value> = take_table(RECENT.name).into_iter().map(|mut r| json!([take(&mut r, "height"), take(&mut r, "hash")])).collect();
    let notes: Map<String, Value> = take_table(NOTES.name)
        .into_iter()
        .map(|mut n| {
            let subaddress = take(&mut n, "subaddress");
            (subaddress.to_string(), take(&mut n, "note"))
        })
        .collect();
    let mut records: Map<String, Value> = Map::new();
    for mut record in take_table(PRICE_RECORDS.name) {
        let Value::String(tx) = take(&mut record, "tx") else { return Err(failed("a price record without its transaction")) };
        if let Value::Array(kept) = records.entry(tx).or_insert_with(|| Value::Array(vec![])) {
            kept.push(Value::Object(record));
        }
    }
    let current = take_table(PRICE_CURRENT.name).pop().map_or(Value::Null, Value::Object);
    let you = take_table(PROFILE.name).pop().map_or(Value::Null, Value::Object);
    let state = json!({
        "recent": recent,
        "outputs": values(take_table(OUTPUTS.name)),
        "spends": values(take_table(SPENDS.name)),
        "sent": values(take_table(SENT.name)),
        "custom_nodes": values(take_table(CUSTOM_NODES.name)),
        "notes": notes,
        "price_records": records,
        "price_cache": {"current": current, "history": values(take_table(PRICE_HISTORY.name))},
        "prices": {
            "currency": take(&mut w, "price_currency"),
            "provider": take(&mut w, "price_provider"),
            "transport": take(&mut w, "price_transport"),
            "fiat_first": take(&mut w, "price_fiat_first"),
        },
        "contacts": {"contacts": values(take_table(CONTACTS.name)), "next_id": take(&mut w, "contacts_next_id")},
        "requests": {"requests": values(take_table(REQUESTS.name)), "next_id": take(&mut w, "requests_next_id")},
        "profile": {"profile": you, "shares": values(take_table(SHARES.name)), "next_id": take(&mut w, "profile_next_id")},
    });
    let Value::Object(mut state) = state else { unreachable!("built as an object") };
    state.append(&mut w);
    Ok(Value::Object(state))
}

fn hash_row(row: &[Sql]) -> u64 {
    let mut h = DefaultHasher::new();
    for value in row {
        match value {
            Sql::Null => 0u8.hash(&mut h),
            Sql::Integer(i) => (1u8, i).hash(&mut h),
            Sql::Text(t) => (2u8, t).hash(&mut h),
            Sql::Real(r) => (3u8, r.to_bits()).hash(&mut h),
            Sql::Blob(b) => (4u8, b).hash(&mut h),
        }
    }
    h.finish()
}

fn quoted(name: &str) -> String {
    format!("\"{name}\"")
}

fn create_table(table: &Table) -> String {
    let columns: Vec<String> = table.columns.iter().map(|(name, kind)| format!("{} {}", quoted(name), sql_type(*kind))).collect();
    format!("CREATE TABLE {} (pos INTEGER PRIMARY KEY, {});\n", quoted(table.name), columns.join(", "))
}

/// An open wallet database. Each row is at its place in the state's list.
pub struct Db {
    connection: Connection,
    /// A hash of each row as on disk, by table, to write only what changed.
    written: HashMap<&'static str, Vec<u64>>,
}

impl Db {
    fn connect(path: &Path, key: &[u8; 32]) -> Result<Connection> {
        let connection = Connection::open(path).map_err(failed)?;
        // SQLCipher leaves freed memory as it was unless told otherwise, and
        // that memory held the key and decrypted pages of keys and history.
        // The setting is process-wide and can only be turned on, so it goes
        // first, before the key is handed over.
        connection.execute_batch("PRAGMA cipher_memory_security = ON;").map_err(failed)?;
        let keyed = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", hex::encode(key)));
        connection.execute_batch(&keyed).map_err(failed)?;
        // The first read is what finds out whether the key is right.
        match connection.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0)) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::NotADatabase => return Err(VaultError::WrongKey),
            Err(e) => return Err(failed(e)),
        }
        // A commit is on the disk when it returns: what's in here is money's
        // history, and saves are few (see `SAVE_EVERY` in the wallet).
        connection.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(())).map_err(failed)?;
        connection.pragma_update(None, "synchronous", "FULL").map_err(failed)?;
        Ok(connection)
    }

    /// A new database at `path`, replacing any there, holding `keys`.
    pub fn create(path: &Path, key: &[u8; 32], keys: &StoredKeys) -> Result<Db> {
        remove_files(path)?;
        let connection = Self::connect(path, key)?;
        let mut schema = String::from("BEGIN;\n");
        schema.push_str("CREATE TABLE keys (id INTEGER PRIMARY KEY CHECK (id = 1), network TEXT NOT NULL, spend TEXT, view TEXT NOT NULL, spend_pub TEXT NOT NULL, seed_kind TEXT, seed_words TEXT, device TEXT);\n");
        for table in TABLES {
            schema.push_str(&create_table(table));
        }
        schema.push_str(&format!("PRAGMA user_version = {VERSION};\nCOMMIT;"));
        connection.execute_batch(&schema).map_err(failed)?;
        keys.write(&connection).map_err(failed)?;
        Ok(Db { connection, written: HashMap::new() })
    }

    /// The database at `path`, brought up to this layout, refusing one with the
    /// wrong key or from a newer Fob.
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Db> {
        let connection = Self::connect(path, key)?;
        let mut version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0)).map_err(failed)?;
        if version == 1 {
            let upgrade = format!("BEGIN;\n{}PRAGMA user_version = 2;\nCOMMIT;", create_table(&CUSTOM_NODES));
            connection.execute_batch(&upgrade).map_err(failed)?;
            version = 2;
        }
        if version == 2 {
            let upgrade = "BEGIN;\nALTER TABLE wallet ADD COLUMN gpu_checked_on TEXT;\nALTER TABLE wallet ADD COLUMN gpu_check_passed INTEGER DEFAULT 0;\nPRAGMA user_version = 3;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 3;
        }
        if version == 3 {
            let upgrade = "BEGIN;\nALTER TABLE wallet ADD COLUMN manual_sync INTEGER DEFAULT 0;\nALTER TABLE wallet ADD COLUMN pull_mode TEXT DEFAULT 'auto';\nALTER TABLE wallet ADD COLUMN incoming_scan_blocks INTEGER DEFAULT 5;\nALTER TABLE wallet ADD COLUMN settled_confirmations INTEGER DEFAULT 10;\nALTER TABLE wallet ADD COLUMN pull_mark_height INTEGER;\nALTER TABLE wallet ADD COLUMN pull_mark_hash TEXT;\nPRAGMA user_version = 4;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 4;
        }
        if version == 4 {
            let upgrade = "BEGIN;\nALTER TABLE sent ADD COLUMN not_confirmed INTEGER DEFAULT 0;\nALTER TABLE sent ADD COLUMN released INTEGER DEFAULT 0;\nPRAGMA user_version = 5;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 5;
        }
        if version == 5 {
            let upgrade = "BEGIN;\nALTER TABLE sent ADD COLUMN signed TEXT;\nALTER TABLE wallet ADD COLUMN node_pin TEXT;\nPRAGMA user_version = 6;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 6;
        }
        if version == 6 {
            // Hardware wallets: which device holds the spend key, and each
            // output's transaction keys, which the device needs to spend it.
            let upgrade = "BEGIN;\nALTER TABLE keys ADD COLUMN device TEXT;\nALTER TABLE outputs ADD COLUMN tx_key TEXT;\nALTER TABLE outputs ADD COLUMN extra_key TEXT;\nPRAGMA user_version = 7;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 7;
        }
        if version == 7 {
            // Light wallet mode: where the wallet syncs from, and what the
            // server has been told. A file from before is a node wallet.
            let upgrade = "BEGIN;\nALTER TABLE wallet ADD COLUMN sync_mode TEXT DEFAULT 'node';\nALTER TABLE wallet ADD COLUMN light_url TEXT;\nALTER TABLE wallet ADD COLUMN light_self_signed INTEGER DEFAULT 0;\nALTER TABLE wallet ADD COLUMN light_pin TEXT;\nALTER TABLE wallet ADD COLUMN light_registered INTEGER DEFAULT 0;\nALTER TABLE wallet ADD COLUMN light_subaddrs_sent INTEGER DEFAULT 0;\nALTER TABLE wallet ADD COLUMN light_scanned_height INTEGER DEFAULT 0;\nPRAGMA user_version = 8;\nCOMMIT;";
            connection.execute_batch(upgrade).map_err(failed)?;
            version = 8;
        }
        if version != VERSION {
            return Err(failed(format!("this wallet's database is version {version}, and this Fob reads version {VERSION}")));
        }
        Ok(Db { connection, written: HashMap::new() })
    }

    pub fn keys(&self) -> Result<StoredKeys> {
        StoredKeys::read(&self.connection).map_err(failed)
    }

    /// The state's JSON form, as saved.
    pub fn load(&self) -> Result<Value> {
        let mut tables = HashMap::new();
        for table in TABLES {
            let names: Vec<String> = table.columns.iter().map(|(name, _)| quoted(name)).collect();
            let sql = format!("SELECT {} FROM {} ORDER BY pos", names.join(", "), quoted(table.name));
            let mut statement = self.connection.prepare(&sql).map_err(failed)?;
            let mut read = statement.query([]).map_err(failed)?;
            let mut rows = vec![];
            while let Some(row) = read.next().map_err(failed)? {
                let mut kept = Map::new();
                for (i, (name, kind)) in table.columns.iter().enumerate() {
                    kept.insert((*name).to_owned(), decode(*kind, row.get(i).map_err(failed)?, name)?);
                }
                rows.push(kept);
            }
            tables.insert(table.name, rows);
        }
        from_rows(tables)
    }

    /// Notes `state` as what's on disk already: the state just loaded.
    pub fn loaded(&mut self, state: Value) -> Result<()> {
        for (table, rows) in to_rows(state)? {
            self.written.insert(table.name, rows.iter().map(|row| hash_row(row)).collect());
        }
        Ok(())
    }

    /// Saves `state`, writing only the rows that changed, all or none.
    pub fn save(&mut self, state: Value) -> Result<()> {
        let tables = to_rows(state)?;
        let mut next: Vec<(&'static str, Vec<u64>)> = Vec::with_capacity(tables.len());
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failed)?;
        for (table, rows) in &tables {
            let was = self.written.get(table.name).map_or(&[][..], Vec::as_slice);
            let hashes: Vec<u64> = rows.iter().map(|row| hash_row(row)).collect();
            let names: Vec<String> = table.columns.iter().map(|(name, _)| quoted(name)).collect();
            let marks = vec!["?"; table.columns.len() + 1].join(", ");
            let sql = format!("INSERT OR REPLACE INTO {} (pos, {}) VALUES ({marks})", quoted(table.name), names.join(", "));
            for (pos, row) in rows.iter().enumerate() {
                if was.get(pos) != Some(&hashes[pos]) {
                    let mut statement = transaction.prepare_cached(&sql).map_err(failed)?;
                    let values = std::iter::once(Sql::Integer(pos as i64)).chain(row.iter().cloned());
                    statement.execute(params_from_iter(values)).map_err(failed)?;
                }
            }
            if was.len() > rows.len() {
                let sql = format!("DELETE FROM {} WHERE pos >= ?", quoted(table.name));
                transaction.execute(&sql, [rows.len() as i64]).map_err(failed)?;
            }
            next.push((table.name, hashes));
        }
        transaction.commit().map_err(failed)?;
        self.written.extend(next);
        Ok(())
    }

    /// Folds the write-ahead log in and closes, leaving one file.
    pub fn close(self) -> Result<()> {
        self.connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").map_err(failed)?;
        self.connection.close().map_err(|(_, e)| failed(e))
    }
}

/// A database's files besides itself.
fn side_files(path: &Path) -> [PathBuf; 2] {
    let beside = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    [beside("-wal"), beside("-shm")]
}

/// Removes a database and its side files, those that exist.
pub fn remove_files(path: &Path) -> Result<()> {
    for file in std::iter::once(path.to_owned()).chain(side_files(path)) {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(failed(e)),
        }
    }
    Ok(())
}

/// Moves a closed database to `to`.
pub fn rename(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Keys;
    use fob_monero::network::Network;

    #[test]
    fn freed_memory_is_wiped_once_a_wallet_is_open() {
        let dir = std::env::temp_dir().join(format!("fob-db-wipe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wallet.db");
        let keys = Keys::generate(Network::Stagenet).keys;
        let db = Db::create(&path, &[7; 32], &StoredKeys::from(&keys)).unwrap();
        let on: String = db.connection.query_row("PRAGMA cipher_memory_security", [], |row| row.get(0)).unwrap();
        assert_eq!(on, "1");
        db.close().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
