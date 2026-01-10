//! Device Drivers

pub mod block;   // Block device abstraction
pub mod console; // Console/TTY driver
pub mod console_input; // STDIN Buffer
pub mod uefi_input;    // UEFI SimpleTextInput Wrapper

/// Initialize drivers
pub fn init() {
    // Initialize serial console (enables IRQ for input)
    console::init();
}
