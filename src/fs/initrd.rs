//! Initial RAM Disk Loading
use alloc::vec::Vec;

/// Embedded Init Binary
static INIT_BIN: &[u8] = include_bytes!("../../init/init.bin");

/// Load initrd into memory/VFS
pub fn load() -> Vec<u8> {
    log::info!("[InitRD] Loading embedded init ({} bytes)...", INIT_BIN.len());
    INIT_BIN.to_vec()
}
