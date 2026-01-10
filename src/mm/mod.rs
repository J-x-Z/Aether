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
        // TEMPORARY DEBUG VERSION: Skip UEFI Memory Map entirely.
        // Use hardcoded "safe" addresses to isolate the crash.
        // ============================================================
        
        // These addresses should be safe on most x86 PCs:
        // - 16MB (0x1000000): Page Table Allocator Start
        // - 32MB (0x2000000): Kernel Heap Start (16MB size)
        // Most PCs have at least 128MB RAM, so this should be safe.
        
        let pt_start: u64 = 0x1000000;   // 16MB
        let pt_size: u64 = 0x1000000;    // 16MB
        let heap_start: u64 = 0x2000000; // 32MB
        let heap_size: u64 = 0x1000000;  // 16MB
        
        // A. Init Bump Allocator (for Page Tables)
        crate::mm::paging::init_allocator(pt_start, pt_size);
        
        // B. Init Global Heap (for Vec/Box)
        unsafe {
            crate::mm::heap::init(heap_start as usize, heap_size as usize);
        }

        crate::mm::paging::init_our_page_tables();
        (paging::PHYS_OFFSET, ())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, ())
    }
}
