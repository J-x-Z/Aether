//! Device Drivers

pub mod block;   // Block device abstraction
pub mod console; // Console/TTY driver

/// Initialize drivers
pub fn init() {
    // TODO: Probe and initialize devices
}
