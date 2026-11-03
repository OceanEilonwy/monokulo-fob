//! What signing a payment costs, apart from the network: Bulletproofs+ for the outputs and
//! a CLSAG for each input (ring of 16). `cargo run --release -p fob-core --example send_bench`.
//! On a phone: build with `cargo ndk -t arm64-v8a build --profile android --example send_bench`.

use std::{hint::black_box, time::Instant};

use monero_wallet::{
    ed25519::{Commitment, Point, Scalar},
    ringct::{
        bulletproofs::Bulletproof,
        clsag::{Clsag, ClsagContext, Decoys},
    },
};
use rand_core::OsRng;
use zeroize::Zeroizing;

fn time<T>(name: &str, runs: u32, mut work: impl FnMut() -> T) {
    black_box(work());
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            black_box(work());
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    println!("{name:<36} median {:>7.1} ms   best {:>7.1} ms", times[times.len() / 2], times[0]);
}

fn random_point() -> Point {
    Point::from(curve25519_dalek::constants::ED25519_BASEPOINT_POINT * Scalar::random(&mut OsRng).into())
}

/// `n` inputs, each hiding among 16 ring members.
fn inputs(n: usize) -> Vec<(Zeroizing<Scalar>, ClsagContext)> {
    (0..n)
        .map(|_| {
            let key = Scalar::random(&mut OsRng);
            let commitment = Commitment::new(Scalar::random(&mut OsRng), 1_000_000_000_000);
            let mut ring: Vec<[Point; 2]> = (0..16).map(|_| [random_point(), random_point()]).collect();
            ring[7] = [Point::from(curve25519_dalek::constants::ED25519_BASEPOINT_POINT * key.into()), commitment.commit()];
            let decoys = Decoys::new((1..=16).map(|i| i * 1000).collect(), 7, ring).unwrap();
            (Zeroizing::new(key), ClsagContext::new(decoys, commitment).unwrap())
        })
        .collect()
}

fn main() {
    println!("-- Bulletproofs+ (what the outputs cost)");
    for outputs in [2usize, 3, 8, 16] {
        let commitments: Vec<Commitment> = (0..outputs).map(|i| Commitment::new(Scalar::random(&mut OsRng), 1_000_000 + i as u64)).collect();
        time(&format!("prove, {outputs} outputs"), 15, || Bulletproof::prove_plus(&mut OsRng, commitments.clone()).unwrap());
    }
    println!("-- CLSAG (what the inputs cost)");
    for n in [1usize, 2, 4, 8] {
        let set = inputs(n);
        time(&format!("sign, {n} input(s), ring 16"), 9, || {
            Clsag::sign(&mut OsRng, set.iter().map(|(k, c)| (k.clone(), c.clone())).collect(), Scalar::random(&mut OsRng), [3u8; 32]).unwrap()
        });
    }
}
