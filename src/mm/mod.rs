//! Memory Management Subsystem

pub mod pmm;     // Physical Memory Manager
pub mod vmm;     // Virtual Memory Manager
pub mod heap;    // Kernel Heap Allocator
pub mod paging;  // Page Table Helpers

use uefi::prelude::*;

/// Initialize memory management
pub fn init(_st: &SystemTable<Boot>) -> (u64, ()) {
    #[cfg(target_arch = "x86_64")]
    {
        // ============================================================
        // DEBUG: Enable ONLY heap init, skip page tables
        // ============================================================
        
        let heap_start: u64 = 0x2000000; // 32MB
        let heap_size: u64 = 0x1000000;  // 16MB
        
        // B. Init Global Heap (for Vec/Box)
        unsafe {
            crate::mm::heap::init(heap_start as usize, heap_size as usize);
        }
        
        // Skip: crate::mm::paging::init_our_page_tables();
        
        (paging::PHYS_OFFSET, ())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, ())
    }
}
