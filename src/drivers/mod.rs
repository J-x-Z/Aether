//! Device Drivers

pub mod block;   // Block device abstraction
pub mod console; // Console/TTY driver
pub mod console_input; // STDIN Buffer

/// Initialize drivers
pub fn init() {
    // TODO: Probe and initialize devices
}
