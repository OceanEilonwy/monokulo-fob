//! The hot crypto, timed: `cargo run --release -p fob-wallet --example crypto_bench`.
//! On a phone: build with `cargo ndk -t arm64-v8a build --profile android --example crypto_bench`,
//! `adb push` the binary to /data/local/tmp and run it, before and after a build-setting change.

use std::{hint::black_box, time::Instant};

use curve25519_dalek::{constants::ED25519_BASEPOINT_POINT, scalar::Scalar};
use sha3::{Digest, Keccak256};

fn time(name: &str, iterations: u32, mut work: impl FnMut()) {
    for _ in 0..iterations / 10 + 1 {
        work();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    let each = start.elapsed().as_secs_f64() / f64::from(iterations);
    println!("{name:<28} {:>9.2} us", each * 1e6);
}

fn main() {
    let small = [7u8; 32];
    let large = vec![9u8; 4096];
    time("keccak256, 32 bytes", 200_000, || {
        black_box(Keccak256::digest(black_box(small)));
    });
    time("keccak256, 4096 bytes", 20_000, || {
        black_box(Keccak256::digest(black_box(&large)));
    });

    let scalar = Scalar::from_bytes_mod_order([3u8; 32]);
    let point = ED25519_BASEPOINT_POINT * Scalar::from_bytes_mod_order([5u8; 32]);
    time("basepoint * scalar", 5_000, || {
        black_box(ED25519_BASEPOINT_POINT * black_box(scalar));
    });
    time("point * scalar (the scan ECDH)", 5_000, || {
        black_box(black_box(point) * black_box(scalar));
    });
}
