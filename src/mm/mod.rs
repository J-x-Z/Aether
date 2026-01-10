//! Memory Management Subsystem

pub mod pmm;     // Physical Memory Manager
pub mod vmm;     // Virtual Memory Manager
pub mod heap;    // Kernel Heap Allocator
pub mod paging;  // Page Table Helpers

use uefi::prelude::*;

/// Initialize memory management
pub fn init(_st: &SystemTable<Boot>) -> (u64, ()) {
    // TODO: Initialize PMM using UEFI memory map
    // For now we use the simple atomic allocator in paging.rs
    
    // Return PHYS_OFFSET for other modules
    #[cfg(target_arch = "x86_64")]
    {
        (paging::PHYS_OFFSET, ())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, ())
    }
}
