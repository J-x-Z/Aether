//! Initial RAM Disk Loading
use alloc::vec::Vec;

/// Embedded Init Binary
static INIT_BIN: &[u8] = include_bytes!("../../init/init.elf");

/// Embedded BusyBox Binary (static musl build)
static BUSYBOX_BIN: &[u8] = include_bytes!("../../init/busybox.bin");

/// Load init binary
pub fn load() -> Vec<u8> {
    log::info!("[InitRD] Loading embedded init ({} bytes)...", INIT_BIN.len());
    INIT_BIN.to_vec()
}

/// Load BusyBox binary
pub fn load_busybox() -> Vec<u8> {
    log::info!("[InitRD] Loading embedded busybox ({} bytes)...", BUSYBOX_BIN.len());
    BUSYBOX_BIN.to_vec()
}
