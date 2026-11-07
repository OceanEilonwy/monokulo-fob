//! Fob's scanning policy. `monokulo-8ar` does what each mode says, given a GPU
//! that's ready, and leaves one choice to its caller: from how many keys Auto
//! hands a batch to the GPU. Without a GPU the CPU does the work, whatever the
//! mode.

/// From this many keys, Auto has the GPU take over from the CPU, as it costs
/// less energy from there. A block has about 57 keys. On a Mali-G715 the CPU
/// took less energy for one block, they were level for ten, and the GPU took
/// half for a hundred, so the GPU takes over at about nine. monokulo-8ar's
/// `examples/scan_speed.rs --tip` measures this.
pub(crate) const AUTO_GPU_FROM: usize = 512;
