//! Settings' Scanning row: what's finding the wallet's money, read-only.

use fob_scanner::{Engine, Reason};

use crate::device;

/// "GPU · Mali-G715", or "CPU · " and why the GPU isn't being used. With
/// `wallet_refused`, the GPU is ready but failed this wallet's own check.
pub fn engine_line(engine: &Engine, wallet_refused: bool) -> String {
    let why = match engine {
        Engine::Gpu { .. } if wallet_refused => "the GPU failed this wallet's check".to_owned(),
        Engine::Gpu { name } => return format!("GPU · {name}"),
        Engine::Cpu { reason } => match reason {
            Reason::NotStarted => "the GPU hasn't started".to_owned(),
            Reason::Starting => "the GPU is starting".to_owned(),
            Reason::NoVulkan => format!("no Vulkan on this {}", device().noun()),
            Reason::NoGpu => format!("no GPU on this {} can do it", device().noun()),
            Reason::FailedSelfTest => "the GPU failed its check".to_owned(),
            Reason::Failed => "the GPU stopped working".to_owned(),
        },
    };
    format!("CPU · {why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_engine_has_its_line() {
        let gpu = Engine::Gpu { name: "Mali-G715".into() };
        assert_eq!(engine_line(&gpu, false), "GPU · Mali-G715");
        assert_eq!(engine_line(&gpu, true), "CPU · the GPU failed this wallet's check");
        let cpu = |reason| engine_line(&Engine::Cpu { reason }, false);
        assert_eq!(cpu(Reason::NotStarted), "CPU · the GPU hasn't started");
        assert_eq!(cpu(Reason::Starting), "CPU · the GPU is starting");
        assert_eq!(cpu(Reason::NoVulkan), "CPU · no Vulkan on this phone");
        assert_eq!(cpu(Reason::NoGpu), "CPU · no GPU on this phone can do it");
        assert_eq!(cpu(Reason::FailedSelfTest), "CPU · the GPU failed its check");
        assert_eq!(cpu(Reason::Failed), "CPU · the GPU stopped working");
        assert_eq!(engine_line(&Engine::Cpu { reason: Reason::Failed }, true), "CPU · the GPU stopped working", "the GPU's own failure says more");
    }
}
