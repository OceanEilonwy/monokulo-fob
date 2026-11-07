//! Fob's scanning policy: the one choice `monokulo-8ar` leaves its caller.

/// Below this many transaction keys, about five mainnet blocks, a batch doesn't
/// go to the GPU and the `Scanner` reads every block instead: a GPU batch has a
/// fixed cost however small (about 5.5 ms on an RX 6800 XT, 46 mJ on a
/// Mali-G715). Decision 362: on the desktop the `Scanner` alone and the GPU
/// break even at about 90 keys, two blocks; on a phone, in energy, the crossover
/// is estimated at three to five blocks but not measured. A wallet caught up at
/// the tip, a block at a time, stays off the GPU either way. fob-wallet's
/// `examples/gpu_crossover.rs` measures it.
pub(crate) const GPU_FROM_KEYS: usize = 256;
