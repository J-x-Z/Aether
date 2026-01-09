//! Paging Support
//! 
//! Platform-specific paging implementations

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    
    /// Recursive page table index (using entry 510 to avoid conflicts with kernel high memory)
    const RECURSIVE_INDEX: usize = 510;
    
    /// Base virtual addresses for recursive page table access
    /// With recursive index 510:
    /// PML4:  0xFFFF_FF7F_BFDF_E000
    /// PDPT:  0xFFFF_FF7F_BFC0_0000 + (pml4_idx * 0x1000)
    /// PD:    0xFFFF_FF7F_8000_0000 + (pml4_idx * 0x200000) + (pdpt_idx * 0x1000)
    /// PT:    0xFFFF_FF00_0000_0000 + (pml4_idx * 0x40000000) + (pdpt_idx * 0x200000) + (pd_idx * 0x1000)
    const PML4_VADDR: u64 = 0xFFFF_FF7F_BFDF_E000;
    
    /// Simple bump allocator for physical frames
    static NEXT_FRAME: AtomicU64 = AtomicU64::new(0x2000000); // 32MB
    
    /// Flag to track if recursive mapping is set up
    static RECURSIVE_SETUP: AtomicBool = AtomicBool::new(false);
    
    fn alloc_frame() -> u64 {
        NEXT_FRAME.fetch_add(4096, Ordering::SeqCst)
    }
    
    /// Set up recursive page table mapping
    /// This maps PML4[RECURSIVE_INDEX] to point to PML4 itself
    pub fn setup_recursive_mapping() {
        if RECURSIVE_SETUP.swap(true, Ordering::SeqCst) {
            return; // Already set up
        }
        
        unsafe {
            let cr3 = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
            
            // UEFI guarantees that the current page tables are accessible
            // We need to find the virtual address of PML4
            // In UEFI, low memory is typically identity-mapped, so we try that first
            // But if CR3 is at a high address, UEFI must have mapped it somewhere
            
            // For UEFI, we assume identity mapping for the first few GB
            // If CR3 is below 1GB, it should be identity-mapped
            if cr3 < 0x40000000 { // 1GB
                let pml4 = cr3 as *mut u64;
                let entry = pml4.add(RECURSIVE_INDEX);
                
                // Set PML4[RECURSIVE_INDEX] = CR3 | PRESENT | WRITABLE
                *entry = cr3 | 0x3; // Don't set USER bit on recursive entry
                
                // Flush TLB
                x86_64::instructions::tlb::flush_all();
                
                log::info!("[Paging] Recursive mapping set up at PML4[{}]", RECURSIVE_INDEX);
            } else {
                // CR3 is at high address, we need to find another way
                // This is unusual for UEFI, log a warning
                log::warn!("[Paging] CR3 at high address 0x{:x}, recursive mapping may fail", cr3);
                
                // Still try - UEFI might have it mapped somewhere
                // Use the physical address directly (this may page fault if not identity-mapped)
                let pml4 = cr3 as *mut u64;
                let entry = pml4.add(RECURSIVE_INDEX);
                *entry = cr3 | 0x3;
                x86_64::instructions::tlb::flush_all();
            }
        }
    }
    
    /// Get virtual address to access a specific page table level using recursive mapping
    /// level: 4=PML4, 3=PDPT, 2=PD, 1=PT
    fn recursive_table_addr(vaddr: u64, level: u8) -> u64 {
        let ri = RECURSIVE_INDEX as u64;
        
        let pml4_idx = (vaddr >> 39) & 0x1FF;
        let pdpt_idx = (vaddr >> 30) & 0x1FF;
        let pd_idx = (vaddr >> 21) & 0x1FF;
        let pt_idx = (vaddr >> 12) & 0x1FF;
        
        // Sign extend for canonical form
        let sign = 0xFFFF_0000_0000_0000u64;
        
        match level {
            4 => sign | (ri << 39) | (ri << 30) | (ri << 21) | (ri << 12),
            3 => sign | (ri << 39) | (ri << 30) | (ri << 21) | (pml4_idx << 12),
            2 => sign | (ri << 39) | (ri << 30) | (pml4_idx << 21) | (pdpt_idx << 12),
            1 => sign | (ri << 39) | (pml4_idx << 30) | (pdpt_idx << 21) | (pd_idx << 12),
            _ => 0,
        }
    }
    
    /// Ensure a page is mapped and accessible to user mode
    unsafe fn map_page_user(vaddr: u64) {
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;
        
        // Access PML4 via recursive mapping
        let pml4_table = recursive_table_addr(vaddr, 4) as *mut u64;
        let pml4_entry = pml4_table.add(pml4_idx);
        
        if *pml4_entry & 1 == 0 {
            // Allocate PDPT
            let frame = alloc_frame();
            // Zero the frame by mapping it temporarily (we'll use it through recursive mapping)
            *pml4_entry = frame | 0x7; // PRESENT | WRITABLE | USER
            x86_64::instructions::tlb::flush_all();
            // Zero the new table
            let pdpt_table = recursive_table_addr(vaddr, 3) as *mut u8;
            core::ptr::write_bytes(pdpt_table, 0, 4096);
        }
        *pml4_entry |= 0x7;
        
        // Access PDPT
        let pdpt_table = recursive_table_addr(vaddr, 3) as *mut u64;
        let pdpt_entry = pdpt_table.add(pdpt_idx);
        
        // Check for 1GB huge page
        if *pdpt_entry & 0x80 != 0 {
            *pdpt_entry |= 0x4; // Set USER bit
            return;
        }
        
        if *pdpt_entry & 1 == 0 {
            let frame = alloc_frame();
            *pdpt_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
            let pd_table = recursive_table_addr(vaddr, 2) as *mut u8;
            core::ptr::write_bytes(pd_table, 0, 4096);
        }
        *pdpt_entry |= 0x7;
        
        // Access PD
        let pd_table = recursive_table_addr(vaddr, 2) as *mut u64;
        let pd_entry = pd_table.add(pd_idx);
        
        // Check for 2MB huge page
        if *pd_entry & 0x80 != 0 {
            *pd_entry |= 0x4;
            return;
        }
        
        if *pd_entry & 1 == 0 {
            let frame = alloc_frame();
            *pd_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
            let pt_table = recursive_table_addr(vaddr, 1) as *mut u8;
            core::ptr::write_bytes(pt_table, 0, 4096);
        }
        *pd_entry |= 0x7;
        
        // Access PT
        let pt_table = recursive_table_addr(vaddr, 1) as *mut u64;
        let pt_entry = pt_table.add(pt_idx);
        
        if *pt_entry & 1 == 0 {
            let frame = alloc_frame();
            *pt_entry = frame | 0x7; // PRESENT | WRITABLE | USER
        } else {
            *pt_entry |= 0x4; // Add USER bit
        }
    }
    
    /// Ensure a range of addresses is accessible to User Mode (Ring 3)
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        if len == 0 { return; }
        
        // Ensure recursive mapping is set up
        setup_recursive_mapping();
        
        let start = start_addr & !0xFFF;
        let end = (start_addr + len + 0xFFF) & !0xFFF;
        
        log::debug!("[Paging] Making 0x{:x}-0x{:x} user accessible", start, end);
        
        let mut addr = start;
        while addr < end {
            unsafe { map_page_user(addr); }
            addr += 4096;
        }
        
        // Final TLB flush
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
