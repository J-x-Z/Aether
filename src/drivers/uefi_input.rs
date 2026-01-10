//! UEFI Input Support
//! 
//! Provides keyboard input through UEFI SimpleTextInput protocol (ConIn)
//! Required for environments like Hyper-V that don't expose PS/2 or serial input

use core::sync::atomic::{AtomicPtr, Ordering};
use uefi::proto::console::text::{Input, Key};
use core::ptr;

/// Global storage for UEFI Input protocol pointer
static UEFI_INPUT_PROTOCOL: AtomicPtr<Input> = AtomicPtr::new(ptr::null_mut());

/// Initialize UEFI input with protocol pointer
/// Safety: Caller must ensure ptr is valid and kept alive (we don't exit boot services so it is)
pub unsafe fn init_protocol(ptr: *mut Input) {
    UEFI_INPUT_PROTOCOL.store(ptr, Ordering::SeqCst);
    log::info!("[Drivers] UEFI Input protocol registered");
}

/// Poll for input from UEFI
/// Should be called regularly (e.g. from timer interrupt)
pub fn poll() {
    let ptr = UEFI_INPUT_PROTOCOL.load(Ordering::SeqCst);
    if ptr.is_null() {
        return;
    }

    // Safety: we assume the pointer is valid as long as we haven't exited boot services
    // and we are single-threaded mostly or holding locks elsewhere if needed.
    // UEFI is not re-entrant, so be careful if calling this from interrupts vs main thread.
    // Ideally only call this from one place.
    let input = unsafe { &mut *ptr };

    // Read key non-blocking
    match input.read_key() {
        Ok(Some(key)) => {
            match key {
                Key::Printable(c) => {
                    let c_u16: u16 = c.into();
                    // Basic ASCII support
                    if c_u16 < 128 {
                        let ch = c_u16 as u8 as char;
                        crate::drivers::console_input::push_char(ch);
                    } else if c_u16 == 0x0D {
                         // Enter key usually returns \r (13), map to \n for convenience or handle in console_input
                         crate::drivers::console_input::push_char('\n');
                    }
                },
                Key::Special(_scan_code) => {
                    // TODO: Handle arrow keys etc if needed
                }
            }
        },
        Ok(None) => {}, // No key pressed
        Err(_) => {},   // Error reading key
    }
}

