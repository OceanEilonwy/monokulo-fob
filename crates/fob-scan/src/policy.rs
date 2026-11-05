//! What each scanning mode does with a batch of keys, given a GPU that's ready.
//! Without one the CPU does the work, whatever the policy; `engine.rs` sees to that.

/// Where a batch of keys goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Cpu,
    Gpu,
}

pub(crate) trait SyncModePolicy: Sync {
    fn route(&self, keys: usize) -> Route;
}

/// Only the GPU.
pub(crate) struct GpuSyncModePolicy;

impl SyncModePolicy for GpuSyncModePolicy {
    fn route(&self, _keys: usize) -> Route {
        Route::Gpu
    }
}

/// Only the CPU's cores.
pub(crate) struct CpuSyncModePolicy;

impl SyncModePolicy for CpuSyncModePolicy {
    fn route(&self, _keys: usize) -> Route {
        Route::Cpu
    }
}

/// Whatever suits the size of the batch.
pub(crate) struct AutoSyncModePolicy {
    /// From this many keys, the GPU takes over from the CPU, as it costs less
    /// energy from there.
    pub(crate) gpu_from: usize,
}

impl AutoSyncModePolicy {
    /// A block has about 57 keys. On a Mali-G715 the CPU took less energy for one
    /// block, they were level for ten, and the GPU took half for a hundred, so the
    /// GPU takes over at about nine. `examples/scan_speed.rs --tip` measures this.
    pub(crate) const CHOSEN: AutoSyncModePolicy = AutoSyncModePolicy { gpu_from: 512 };
}

impl SyncModePolicy for AutoSyncModePolicy {
    fn route(&self, keys: usize) -> Route {
        if keys < self.gpu_from { Route::Cpu } else { Route::Gpu }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_goes_by_the_size_of_the_batch() {
        let auto = AutoSyncModePolicy { gpu_from: 100 };
        assert_eq!(auto.route(0), Route::Cpu);
        assert_eq!(auto.route(99), Route::Cpu);
        assert_eq!(auto.route(100), Route::Gpu);
        assert_eq!(auto.route(1_000_000), Route::Gpu);
    }

    #[test]
    fn the_others_go_one_way_at_any_size() {
        for keys in [0, 1, 1_000_000] {
            assert_eq!(GpuSyncModePolicy.route(keys), Route::Gpu);
            assert_eq!(CpuSyncModePolicy.route(keys), Route::Cpu);
        }
    }
}
