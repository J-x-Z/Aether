//! Paging Support
//! 
//! Platform-specific paging implementations

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use x86_64::structures::paging::{PageTable, PageTableFlags};
    use x86_64::PhysAddr;
    use core::sync::atomic::{AtomicU64, Ordering};
    
    /// Simple bump allocator for physical frames
    /// UEFI typically identity-maps first 1-4GB, so we stay in that range
    /// Start at 32MB to be safe from UEFI reserved areas
    static NEXT_FRAME: AtomicU64 = AtomicU64::new(0x2000000); // 32MB
    
    fn alloc_frame() -> u64 {
        let addr = NEXT_FRAME.fetch_add(4096, Ordering::SeqCst);
        // Zero the frame (important for new page tables)
        unsafe {
            core::ptr::write_bytes(addr as *mut u8, 0, 4096);
        }
        addr
    }
    
    /// Get or create a page table entry, allocating intermediate tables as needed
    /// Returns a mutable reference to the final page table entry
    unsafe fn get_or_create_pte(vaddr: u64) -> *mut u64 {
        let pml4_addr = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;
        
        // PML4 -> PDPT
        let pml4 = pml4_addr as *mut u64;
        let pml4_entry = pml4.add(pml4_idx);
        
        let pdpt_addr = if *pml4_entry & 1 == 0 {
            // Not present, allocate new PDPT
            let new_pdpt = alloc_frame();
            *pml4_entry = new_pdpt | 0x7; // PRESENT | WRITABLE | USER
            new_pdpt
        } else {
            *pml4_entry & !0xFFF
        };
        
        // Ensure USER bit is set on PML4 entry
        *pml4_entry |= 0x7; // PRESENT | WRITABLE | USER
        
        // PDPT -> PD
        let pdpt = pdpt_addr as *mut u64;
        let pdpt_entry = pdpt.add(pdpt_idx);
        
        // Check for 1GB huge page
        if *pdpt_entry & 0x80 != 0 {
            // This is a 1GB page, we can't subdivide it easily
            // Just set USER bit and return
            *pdpt_entry |= 0x4; // USER
            return pdpt_entry;
        }
        
        let pd_addr = if *pdpt_entry & 1 == 0 {
            let new_pd = alloc_frame();
            *pdpt_entry = new_pd | 0x7;
            new_pd
        } else {
            *pdpt_entry & !0xFFF
        };
        
        *pdpt_entry |= 0x7;
        
        // PD -> PT
        let pd = pd_addr as *mut u64;
        let pd_entry = pd.add(pd_idx);
        
        // Check for 2MB huge page
        if *pd_entry & 0x80 != 0 {
            *pd_entry |= 0x4; // USER
            return pd_entry;
        }
        
        let pt_addr = if *pd_entry & 1 == 0 {
            let new_pt = alloc_frame();
            *pd_entry = new_pt | 0x7;
            new_pt
        } else {
            *pd_entry & !0xFFF
        };
        
        *pd_entry |= 0x7;
        
        // Return pointer to PT entry
        let pt = pt_addr as *mut u64;
        pt.add(pt_idx)
    }
    
    /// Ensure a range of addresses is accessible to User Mode (Ring 3)
    /// This allocates new pages if needed and sets USER bit on all levels
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        if len == 0 { return; }
        
        let start = start_addr & !0xFFF; // Page align
        let end = (start_addr + len + 0xFFF) & !0xFFF;
        
        log::debug!("[Paging] Making 0x{:x}-0x{:x} user accessible", start, end);
        
        let mut addr = start;
        while addr < end {
            unsafe {
                let pte = get_or_create_pte(addr);
                
                if *pte & 1 == 0 {
                    // Not present, allocate a new frame
                    let frame = alloc_frame();
                    *pte = frame | 0x7; // PRESENT | WRITABLE | USER
                    log::trace!("[Paging] Mapped 0x{:x} -> 0x{:x}", addr, frame);
                } else {
                    // Already present, just add USER bit
                    *pte |= 0x4; // USER
                }
            }
            addr += 4096;
        }
        
        // Flush TLB
        unsafe {
            x86_64::instructions::tlb::flush_all();
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod aarch64_paging {
    /// Ensure a range of addresses is accessible to EL0 (userspace)
    /// TODO: Implement proper ARM64 page table manipulation
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        log::info!(
            "[MMU] ARM64: Marking 0x{:x}-0x{:x} as user accessible (stub)",
            start_addr,
            start_addr + len
        );
        // ARM64 uses TTBR0_EL1 for user addresses and TTBR1_EL1 for kernel addresses.
        // UEFI gives us identity mapping, which we use for now.
        // TODO: Walk page tables and set AP bits for user access
    }
}

// Re-export the correct implementation
#[cfg(target_arch = "x86_64")]
pub use x86_64_paging::*;

#[cfg(target_arch = "aarch64")]
pub use aarch64_paging::*;
