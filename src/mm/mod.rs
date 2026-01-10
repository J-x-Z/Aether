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
        // FULL INIT: Heap + Page Tables (needed for make_user_accessible)
        // ============================================================
        
        let pt_start: u64 = 0x1000000;   // 16MB - for page table structures
        let pt_size: u64 = 0x1000000;    // 16MB
        let heap_start: u64 = 0x2000000; // 32MB
        let heap_size: u64 = 0x1000000;  // 16MB
        
        // A. Init Bump Allocator for Page Tables
        crate::mm::paging::init_allocator(pt_start, pt_size);
        
        // B. Init Global Heap (for Vec/Box)
        unsafe {
            crate::mm::heap::init(heap_start as usize, heap_size as usize);
        }
        
        // C. Initialize our own page tables (REQUIRED for make_user_accessible!)
        crate::mm::paging::init_our_page_tables();
        
        (paging::PHYS_OFFSET, ())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, ())
    }
}
