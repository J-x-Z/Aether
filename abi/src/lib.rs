#![no_std]

#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyperCall {
    Print = 0,
    Exit = 1,
    // Future:
    // DrawFrame = 2,
    // Sleep = 3,
}

impl HyperCall {
    pub fn from_u64(val: u64) -> Option<Self> {
        match val {
            0 => Some(Self::Print),
            1 => Some(Self::Exit),
            _ => None,
        }
    }
}

pub mod mmio {
    pub const RAM_SIZE: usize = 16 * 1024 * 1024; // 16MB
    pub const FB_ADDR: usize = 0x100000;          // 1MB offset
    pub const DISK_ADDR: usize = 0x300000;        // 3MB offset
    pub const KEYBOARD_STATUS: usize = 0x80000;
    pub const KEYBOARD_DATA: usize = 0x80004;
}
