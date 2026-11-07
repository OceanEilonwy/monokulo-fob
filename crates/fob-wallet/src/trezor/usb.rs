//! A Trezor on a computer's USB port, through libusb (`rusb`): the desktop's
//! [`Link`]. Android has its own, over its USB host API. Only trezor-core
//! devices (Model T, Safe 3, Safe 5) hold Monero; they all show up as one
//! USB product, with the WebUSB interface first.
//!
//! On Linux the device has to be readable by the person: Trezor's udev rules
//! (`51-trezor.rules`, which Trezor Suite installs) give that. Without them
//! opening fails with a permissions error, said in words here.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use rusb::{Direction, TransferType, UsbContext};

use super::Link;

const VENDOR: u16 = 0x1209;
const PRODUCT: u16 = 0x53c1;
const REPORT: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(3);
/// Each wait on the device, so a closed link is noticed soon.
const READ_SLICE: Duration = Duration::from_millis(300);
/// Reading gives up after this: five minutes to find the device and press.
const READ_PATIENCE: Duration = Duration::from_secs(5 * 60);

/// An opened Trezor. Released when dropped, so another app can have it.
pub struct UsbLink {
    handle: rusb::DeviceHandle<rusb::Context>,
    interface: u8,
    input: u8,
    output: u8,
    interrupt: bool,
    closed: AtomicBool,
}

/// Whether a Trezor is plugged in, without opening it.
pub fn present() -> bool {
    rusb::Context::new().and_then(|c| c.devices()).is_ok_and(|devices| {
        devices.iter().any(|d| d.device_descriptor().is_ok_and(|desc| desc.vendor_id() == VENDOR && desc.product_id() == PRODUCT))
    })
}

impl UsbLink {
    /// Opens the Trezor plugged in, or says why not.
    pub fn open() -> Result<UsbLink, String> {
        let context = rusb::Context::new().map_err(|e| format!("USB isn't available here: {e}"))?;
        let devices = context.devices().map_err(|e| format!("USB devices can't be listed: {e}"))?;
        let device = devices
            .iter()
            .find(|d| d.device_descriptor().is_ok_and(|desc| desc.vendor_id() == VENDOR && desc.product_id() == PRODUCT))
            .ok_or_else(|| "No Trezor is plugged in. Connect it with a USB cable and unlock it. (Trezor Model One can't hold Monero.)".to_string())?;
        let config = device.active_config_descriptor().map_err(|e| format!("the Trezor's USB layout can't be read: {e}"))?;
        // The first interface with a way in and a way out is the one to talk to.
        let mut chosen = None;
        for interface in config.interfaces() {
            for descriptor in interface.descriptors() {
                let endpoints: Vec<_> = descriptor.endpoint_descriptors().filter(|e| matches!(e.transfer_type(), TransferType::Interrupt | TransferType::Bulk)).collect();
                let input = endpoints.iter().find(|e| e.direction() == Direction::In);
                let output = endpoints.iter().find(|e| e.direction() == Direction::Out);
                if let (Some(input), Some(output)) = (input, output) {
                    chosen = Some((descriptor.interface_number(), input.address(), output.address(), input.transfer_type() == TransferType::Interrupt));
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        let (interface, input, output, interrupt) = chosen.ok_or_else(|| "This Trezor shows no way to talk to it.".to_string())?;
        let handle = device.open().map_err(|e| match e {
            rusb::Error::Access => "Fob isn't allowed to use the Trezor. On Linux, install Trezor's udev rules (Trezor Suite does), then plug it in again.".to_string(),
            rusb::Error::Busy => "Another app is using the Trezor. Close Trezor Suite or the other wallet and try again.".to_string(),
            e => format!("the Trezor couldn't be opened: {e}"),
        })?;
        // A kernel driver holding the interface (rare: the WebUSB one has none) is let go for the while.
        let _ = handle.set_auto_detach_kernel_driver(true);
        handle.claim_interface(interface).map_err(|e| match e {
            rusb::Error::Busy => "Another app is using the Trezor. Close Trezor Suite or the other wallet and try again.".to_string(),
            e => format!("the Trezor's interface couldn't be claimed: {e}"),
        })?;
        Ok(UsbLink { handle, interface, input, output, interrupt, closed: AtomicBool::new(false) })
    }

    /// Ends any wait on the device: the next read fails at once.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Link for UsbLink {
    fn write(&self, report: &[u8]) -> Result<(), String> {
        if self.closed.load(Ordering::SeqCst) {
            return Err("the Trezor was let go".into());
        }
        let sent = if self.interrupt { self.handle.write_interrupt(self.output, report, WRITE_TIMEOUT) } else { self.handle.write_bulk(self.output, report, WRITE_TIMEOUT) };
        match sent {
            Ok(n) if n == report.len() => Ok(()),
            Ok(_) => Err("the Trezor didn't take all of what Fob sent".into()),
            Err(rusb::Error::NoDevice) => Err("the Trezor was unplugged".into()),
            Err(e) => Err(format!("the Trezor didn't take what Fob sent: {e}")),
        }
    }

    fn read(&self) -> Result<Vec<u8>, String> {
        let mut report = [0u8; REPORT];
        let deadline = Instant::now() + READ_PATIENCE;
        while Instant::now() < deadline {
            if self.closed.load(Ordering::SeqCst) {
                return Err("cancelled".into());
            }
            let got = if self.interrupt { self.handle.read_interrupt(self.input, &mut report, READ_SLICE) } else { self.handle.read_bulk(self.input, &mut report, READ_SLICE) };
            match got {
                Ok(0) | Err(rusb::Error::Timeout) => continue,
                Ok(n) => return Ok(report[..n].to_vec()),
                Err(rusb::Error::NoDevice) => return Err("the Trezor was unplugged".into()),
                Err(e) => return Err(format!("the Trezor stopped answering: {e}")),
            }
        }
        Err("the Trezor didn't answer in time".into())
    }
}

impl Drop for UsbLink {
    fn drop(&mut self) {
        self.close();
        let _ = self.handle.release_interface(self.interface);
    }
}
