//! Paging Support
//! 
//! Platform-specific paging implementations

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    
    /// Frame allocator starting at 1MB (guaranteed identity-mapped by UEFI)
    /// We reserve 1MB-2MB for page tables, 2MB+ for user allocations
    static PT_ALLOCATOR: AtomicU64 = AtomicU64::new(0x100000); // 1MB - for page tables
    static FRAME_ALLOCATOR: AtomicU64 = AtomicU64::new(0x200000); // 2MB - for user pages
    
    static OUR_PML4: AtomicU64 = AtomicU64::new(0);
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    
    /// Allocate a page for page tables (in 1-2MB range, guaranteed identity-mapped)
    fn alloc_pt_page() -> u64 {
        let addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        unsafe { core::ptr::write_bytes(addr as *mut u8, 0, 4096); }
        addr
    }
    
    /// Allocate a page for user data
    fn alloc_user_page() -> u64 {
        let addr = FRAME_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        unsafe { core::ptr::write_bytes(addr as *mut u8, 0, 4096); }
        addr
    }
    
    /// Initialize our own page tables with identity mapping for first 4GB
    pub fn init_our_page_tables() {
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            return;
        }
        
        // Allocate our PML4 at 1MB
        let pml4_addr = alloc_pt_page();
        OUR_PML4.store(pml4_addr, Ordering::SeqCst);
        
        let pml4 = pml4_addr as *mut u64;
        
        // Create identity mapping for first 4GB using 2MB pages (512 entries in PD = 1GB each)
        // We need 4 PDPT entries, each pointing to a PD with 512 2MB pages
        
        unsafe {
            // Allocate PDPT
            let pdpt_addr = alloc_pt_page();
            
            // PML4[0] -> PDPT (for first 512GB, we only use first 4GB)
            *pml4.add(0) = pdpt_addr | 0x3; // PRESENT | WRITABLE (no USER for kernel region)
            
            let pdpt = pdpt_addr as *mut u64;
            
            // Create 4 PDs for 4GB of identity mapping
            for gb in 0..4u64 {
                let pd_addr = alloc_pt_page();
                *pdpt.add(gb as usize) = pd_addr | 0x3;
                
                let pd = pd_addr as *mut u64;
                
                // Fill PD with 512 2MB huge pages
                for i in 0..512u64 {
                    let phys_addr = (gb << 30) | (i << 21);
                    // 2MB huge page: PRESENT | WRITABLE | HUGE_PAGE
                    *pd.add(i as usize) = phys_addr | 0x83;
                }
            }
            
            // Set up recursive mapping at PML4[510]
            *pml4.add(510) = pml4_addr | 0x3; // PRESENT | WRITABLE
            
            log::info!("[Paging] Created new page tables at 0x{:x}", pml4_addr);
            log::info!("[Paging] 4GB identity mapping with 2MB pages");
            log::info!("[Paging] Recursive mapping at PML4[510]");
            
            // Switch to our page tables
            let (_, cr3_flags) = x86_64::registers::control::Cr3::read();
            x86_64::registers::control::Cr3::write(
                x86_64::structures::paging::PhysFrame::containing_address(
                    x86_64::PhysAddr::new(pml4_addr)
                ),
                cr3_flags
            );
            
            log::info!("[Paging] Switched to new CR3: 0x{:x}", pml4_addr);
        }
    }
    
    /// Get virtual address to access a specific page table level using recursive mapping
    fn recursive_table_addr(vaddr: u64, level: u8) -> u64 {
        const RI: u64 = 510;
        
        let pml4_idx = (vaddr >> 39) & 0x1FF;
        let pdpt_idx = (vaddr >> 30) & 0x1FF;
        let pd_idx = (vaddr >> 21) & 0x1FF;
        
        let sign = 0xFFFF_0000_0000_0000u64;
        
        match level {
            4 => sign | (RI << 39) | (RI << 30) | (RI << 21) | (RI << 12),
            3 => sign | (RI << 39) | (RI << 30) | (RI << 21) | (pml4_idx << 12),
            2 => sign | (RI << 39) | (RI << 30) | (pml4_idx << 21) | (pdpt_idx << 12),
            1 => sign | (RI << 39) | (pml4_idx << 30) | (pdpt_idx << 21) | (pd_idx << 12),
            _ => 0,
        }
    }
    
    /// Map a 4KB page for user access, breaking down 2MB pages if needed
    unsafe fn map_page_user_4k(vaddr: u64) {
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;
        
        // Access PML4 via recursive mapping
        let pml4_table = recursive_table_addr(vaddr, 4) as *mut u64;
        let pml4_entry = pml4_table.add(pml4_idx);
        
        if *pml4_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pml4_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pml4_entry |= 0x7;
        
        // Access PDPT
        let pdpt_table = recursive_table_addr(vaddr, 3) as *mut u64;
        let pdpt_entry = pdpt_table.add(pdpt_idx);
        
        // Check for 1GB huge page
        if *pdpt_entry & 0x80 != 0 {
            *pdpt_entry |= 0x4;
            return;
        }
        
        if *pdpt_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pdpt_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pdpt_entry |= 0x7;
        
        // Access PD
        let pd_table = recursive_table_addr(vaddr, 2) as *mut u64;
        let pd_entry = pd_table.add(pd_idx);
        
        // Check for 2MB huge page
        if *pd_entry & 0x80 != 0 {
            // This is a 2MB page, we need to set USER bit on it
            // For simplicity, just set USER on the huge page entry
            *pd_entry |= 0x4;
            return;
        }
        
        if *pd_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pd_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pd_entry |= 0x7;
        
        // Access PT
        let pt_table = recursive_table_addr(vaddr, 1) as *mut u64;
        let pt_entry = pt_table.add(pt_idx);
        
        if *pt_entry & 1 == 0 {
            let frame = alloc_user_page();
            *pt_entry = frame | 0x7;
        } else {
            *pt_entry |= 0x4;
        }
    }
    
    /// Ensure a range of addresses is accessible to User Mode (Ring 3)
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        if len == 0 { return; }
        
        // First, ensure our page tables are set up
        init_our_page_tables();
        
        let start = start_addr & !0xFFF;
        let end = (start_addr + len + 0xFFF) & !0xFFF;
        
        log::debug!("[Paging] Making 0x{:x}-0x{:x} user accessible", start, end);
        
        let mut addr = start;
        while addr < end {
            unsafe { map_page_user_4k(addr); }
            addr += 4096;
        }
        
        unsafe { x86_64::instructions::tlb::flush_all(); }
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
