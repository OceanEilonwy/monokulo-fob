//! Which engine does the arithmetic: the setting, and the one GPU a process has.

use std::{
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use zeroize::Zeroizing;

use crate::{
    fe::Digits,
    gpu::{Gpu, Points, TUNING},
    policy::{AutoSyncModePolicy, CpuSyncModePolicy, GpuSyncModePolicy, Route, SyncModePolicy},
    vectors,
};

/// What scans. A GPU that's still starting, that failed its test vectors, or
/// that's failed or absent, leaves the CPU doing the work in every mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The GPU, for everything.
    Gpu,
    /// Whatever suits the size of the batch: the default. See [`AutoSyncModePolicy`].
    Auto,
    /// Only the CPU's cores.
    Cpu,
}

static MODE: AtomicU8 = AtomicU8::new(1);
static CACHE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Set once a GPU has failed, its test vectors included: it isn't tried again
/// until the app next starts.
static BROKEN: AtomicBool = AtomicBool::new(false);
static STARTING: AtomicBool = AtomicBool::new(false);
static GPU: OnceLock<Mutex<Option<Gpu>>> = OnceLock::new();

pub fn set_mode(mode: Mode) {
    MODE.store(
        match mode {
            Mode::Gpu => 0,
            Mode::Auto => 1,
            Mode::Cpu => 2,
        },
        Ordering::Relaxed,
    );
    if mode != Mode::Cpu {
        warm_up();
    }
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        0 => Mode::Gpu,
        1 => Mode::Auto,
        _ => Mode::Cpu,
    }
}

/// Where compiled shaders are kept: the app's cache folder.
pub fn set_cache_dir(dir: PathBuf) {
    *CACHE_DIR.lock().expect("cache lock") = Some(dir);
}

/// Starts the GPU on a thread of its own, if it's not been: compiles the shader,
/// then has it pass the test vectors before anything else is asked of it. It
/// starts once per process, so this is the check on each cold start of the app.
pub fn warm_up() {
    if BROKEN.load(Ordering::Relaxed) || GPU.get().is_some() || STARTING.swap(true, Ordering::Relaxed) {
        return;
    }
    let dir = CACHE_DIR.lock().expect("cache lock").clone();
    let started = std::thread::Builder::new().name("fob-scan-gpu".into()).spawn(move || match Gpu::start(dir, TUNING, false) {
        Ok(gpu) if matches!(vectors::passes(&gpu), Ok(true)) => {
            let _ = GPU.set(Mutex::new(Some(gpu)));
        }
        _ => BROKEN.store(true, Ordering::Relaxed),
    });
    if started.is_err() {
        BROKEN.store(true, Ordering::Relaxed);
    }
}

/// Whether the GPU has failed or can't be used, so the CPU is scanning whatever
/// the mode says. It's false while the GPU is still starting.
pub fn gpu_failed() -> bool {
    BROKEN.load(Ordering::Relaxed)
}

/// Whether the GPU has started, passed its vectors and hasn't failed.
pub fn gpu_ready() -> bool {
    !BROKEN.load(Ordering::Relaxed) && GPU.get().is_some()
}

/// The GPU and driver that's ready to scan, if one is.
pub fn gpu_identity() -> Option<String> {
    if !gpu_ready() {
        return None;
    }
    GPU.get()?.lock().ok()?.as_ref().map(Gpu::identity)
}

/// Has the ready GPU work with a wallet's own view key, for that wallet to keep
/// the result. `None` if there's no GPU to ask, or it failed, which retires it.
pub fn check_wallet(view: &Zeroizing<[u8; 32]>) -> Option<bool> {
    let slot = GPU.get()?;
    let mut gpu = slot.lock().ok()?;
    match vectors::passes_for_wallet(gpu.as_ref()?, view) {
        Ok(passed) => Some(passed),
        Err(_) => {
            *gpu = None;
            BROKEN.store(true, Ordering::Relaxed);
            None
        }
    }
}

/// Where `keys` go. `gpu_allowed` is the wallet's say; the GPU still warms up without it.
pub(crate) fn route(keys: usize, gpu_allowed: bool) -> Route {
    route_for(mode(), keys, usable() && gpu_allowed)
}

fn policy(mode: Mode) -> &'static dyn SyncModePolicy {
    match mode {
        Mode::Gpu => &GpuSyncModePolicy,
        Mode::Auto => &AutoSyncModePolicy::CHOSEN,
        Mode::Cpu => &CpuSyncModePolicy,
    }
}

fn usable() -> bool {
    if BROKEN.load(Ordering::Relaxed) {
        return false;
    }
    if GPU.get().is_none() {
        if mode() != Mode::Cpu {
            warm_up();
        }
        return false;
    }
    true
}

fn route_for(mode: Mode, keys: usize, gpu_ready: bool) -> Route {
    if gpu_ready { policy(mode).route(keys) } else { Route::Cpu }
}

/// The most keys to give [`derive`] at once.
pub(crate) fn batch() -> usize {
    TUNING.batch
}

/// 8aR on the GPU, or `None` once it's failed, which retires it.
pub(crate) fn derive(digits: &Digits, keys: &[[u8; 32]]) -> Option<Points> {
    let slot = GPU.get()?;
    let mut gpu = slot.lock().ok()?;
    match gpu.as_ref()?.derive(digits, keys) {
        Ok(points) => Some(points),
        Err(_) => {
            *gpu = None;
            BROKEN.store(true, Ordering::Relaxed);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_gpu(route: Route) -> Option<bool> {
        match route {
            Route::Cpu => None,
            Route::Gpu { cpu_too } => Some(cpu_too),
        }
    }

    #[test]
    fn each_mode_goes_where_it_says() {
        assert_eq!(is_gpu(route_for(Mode::Cpu, 100_000, true)), None);
        assert_eq!(is_gpu(route_for(Mode::Gpu, 1, true)), Some(false), "the GPU, even for a little");
        assert_eq!(is_gpu(route_for(Mode::Gpu, 100_000, true)), Some(false), "and only the GPU for a lot");
        assert_eq!(is_gpu(route_for(Mode::Auto, 1, true)), None, "a little: the CPU costs less");
        assert_eq!(is_gpu(route_for(Mode::Auto, AutoSyncModePolicy::CHOSEN.gpu_from, true)), Some(false));
        assert_eq!(is_gpu(route_for(Mode::Auto, AutoSyncModePolicy::CHOSEN.both_from, true)), Some(true), "a lot of blocks: the cores too");
        for mode in [Mode::Auto, Mode::Gpu, Mode::Cpu] {
            assert_eq!(is_gpu(route_for(mode, 100_000, false)), None, "no GPU, no choice");
        }
    }
}
