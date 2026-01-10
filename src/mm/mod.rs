//! Memory Management Subsystem

pub mod pmm;     // Physical Memory Manager
pub mod vmm;     // Virtual Memory Manager
pub mod heap;    // Kernel Heap Allocator
pub mod paging;  // Page Table Helpers

use uefi::prelude::*;

/// Initialize memory management
pub fn init(st: &SystemTable<Boot>) -> (u64, ()) {
    #[cfg(target_arch = "x86_64")]
    {
        use uefi::table::boot::MemoryType;
        
        log::info!("[MM] Getting UEFI Memory Map...");
        
        // 1. Get Memory Map Size
        let map_size = st.boot_services().memory_map_size().map_size;
        // Allocate with padding for fragmentation/alignment
        let buffer_size = map_size + 8 * core::mem::size_of::<uefi::table::boot::MemoryDescriptor>();
        
        let mut buffer = alloc::vec![0u8; buffer_size];
        
        // 2. Get Memory Map
        // We need to collect the best region, then drop the iterator/map to satisfy borrow checker if needed,
        // (though loop logic usually works).
        let mut max_size = 0u64;
        let mut best_start = 0u64;
        
        // Scope to drop the borrow on BootServices
        {
            let map = st.boot_services().memory_map(&mut buffer).expect("Failed to get memory map");
            
            for desc in map.entries() {
                if desc.ty == (MemoryType::CONVENTIONAL) {
                    if desc.page_count > 0 {
                        let size = desc.page_count * 4096;
                        // log::trace!("[MM] Found RAM: 0x{:x} ({} KB)", desc.phys_start, size / 1024);
                        
                        // We need a contiguous chunk for our simple bump allocator.
                        // Choose the largest one.
                        // Filter out very low memory (< 1MB) which might be quirky.
                        // CRITICAL: Filter out memory > 4GB because our current paging only maps first 4GB!
                        let end = desc.phys_start + size;
                        if size > max_size && desc.phys_start >= 0x100000 && end < 0x100000000 {
                            max_size = size;
                            best_start = desc.phys_start;
                        }
                    }
                }
            }
        }
        
        if max_size > 0 {
            log::info!("[MM] Largest Free RAM: 0x{:x} ({} MB)", best_start, max_size / 1024 / 1024);
            
            // 3. Initialize Allocator with safe region
            
            // WE NEED TO SPLIT THE MEMORY:
            // Region 1: Bump Allocator for Page Tables (needs ~16MB+)
            // Region 2: Generic Heap (Vec, Box, etc) (needs rest)
            
            let pt_size = 16 * 1024 * 1024; // 16MB for Paging Structures
            if max_size < pt_size * 2 {
                log::error!("[MM] Not enough RAM for Heap+Paging!");
            }
            
            let pt_start = best_start;
            let heap_start = best_start + pt_size;
            let heap_size = max_size - pt_size;
            
            // A. Init Bump Allocator (for Page Tables)
            crate::mm::paging::init_allocator(pt_start, pt_size);
            
            // B. Init Global Heap (for Vec/Box)
            unsafe {
                crate::mm::heap::init(heap_start as usize, heap_size as usize);
            }
        } else {
            log::error!("[MM] CRITICAL: No Conventional Memory found!");
            // Fallback to defaults (will likely crash)
        }

        crate::mm::paging::init_our_page_tables();
        (paging::PHYS_OFFSET, ())
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        (0, ())
    }
}
