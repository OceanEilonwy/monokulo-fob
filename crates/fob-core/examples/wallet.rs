//! A command-line Fob wallet, for trying fob-core against stagenet:
//!
//! ```text
//! cargo run --release -p fob-core --example wallet -- <file> address
//! cargo run --release -p fob-core --example wallet -- <file> sync
//! cargo run --release -p fob-core --example wallet -- <file> send <address> <amount>
//! ```
//!
//! The file is made on first use (a new stagenet wallet). Its key is fixed:
//! this is for test coins only.

use std::path::Path;

use fob_core::{Network, Wallet, amount};

const KEY: [u8; 32] = [0x46; 32];

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = Path::new(&args[0]);
    let wallet = if path.exists() { Wallet::open(path, KEY).unwrap() } else { Wallet::create(path, KEY, Network::Stagenet).unwrap() };
    match args.get(1).map(String::as_str) {
        Some("address") => println!("{}", wallet.primary_address()),
        Some("sync") => {
            while wallet.sync_step().await.unwrap() {}
            let b = wallet.balance().await;
            println!("synced to {}: total {} unlocked {}", wallet.status().scanned, amount::format(b.total), amount::format(b.unlocked));
            for a in wallet.activity().await {
                println!("{:?} {} {} conf {}", a.direction, amount::format(a.amount), a.tx, a.confirmations);
            }
        }
        Some("send") => {
            while wallet.sync_step().await.unwrap() {}
            let prepared = wallet.prepare(&args[2], amount::parse(&args[3]).unwrap(), None, None).await.unwrap();
            println!("fee {}", amount::format(prepared.fee));
            println!("sent {}", wallet.send(prepared.id).await.unwrap());
        }
        _ => eprintln!("address | sync | send <address> <amount>"),
    }
}
