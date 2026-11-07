//! Fob's scanning policy: the one choice `monokulo-8ar` leaves its caller.

/// Below this many transaction keys, about nine blocks, a batch doesn't go to
/// the GPU and the `Scanner` reads every block instead: a GPU batch has a fixed
/// cost however small (46 mJ on a Mali-G715, 3.7 ms on an RX 6800 XT). The
/// number is decision 332's, measured against the CPU prefilter that's since
/// gone (decision 360); the `Scanner` alone costs more per block than that did,
/// so the true crossover is likely lower. monokulo-8ar's
/// `examples/scan_speed.rs --tip` measures the GPU's side.
pub(crate) const GPU_FROM_KEYS: usize = 512;
