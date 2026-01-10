//! Paging Support
//! 
//! Platform-specific paging implementations
//! Uses OFFSET MAPPING (Linux-style direct map) instead of recursive page tables

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    
    /// Physical-to-Virtual offset for kernel direct mapping
    /// All physical memory is accessible at PHYS_OFFSET + phys_addr
    /// This maps physical memory starting at virtual address 0xFFFF_8000_0000_0000
    pub const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000;
    
    /// Convert physical address to virtual (kernel direct map)
    #[inline]
    pub fn phys_to_virt(phys: u64) -> *mut u8 {
        (phys.wrapping_add(PHYS_OFFSET)) as *mut u8
    }
    
    /// Convert virtual address to physical
    #[inline]
    pub fn virt_to_phys(virt: u64) -> u64 {
        virt.wrapping_sub(PHYS_OFFSET)
    }
    
    /// Frame allocator - at 64MB to avoid overwriting Kernel Code/Data
    static PT_ALLOCATOR: AtomicU64 = AtomicU64::new(0x4000000); // 64MB - for page tables
    static FRAME_ALLOCATOR: AtomicU64 = AtomicU64::new(0x4100000); // 65MB - for user pages
    
    static OUR_PML4: AtomicU64 = AtomicU64::new(0);
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    
    /// Allocate a page for page tables (zeroed)
    fn alloc_pt_page() -> u64 {
        let addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        // Use identity mapping during init, offset mapping after
        let ptr = if INITIALIZED.load(Ordering::SeqCst) {
            phys_to_virt(addr)
        } else {
            addr as *mut u8
        };
        unsafe { core::ptr::write_bytes(ptr, 0, 4096); }
        addr
    }
    
    /// Allocate a page for user data
    fn alloc_user_page() -> u64 {
        let addr = FRAME_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        let ptr = phys_to_virt(addr);
        unsafe { core::ptr::write_bytes(ptr, 0, 4096); }
        addr
    }
    
    /// Initialize page tables with identity mapping AND kernel direct mapping
    /// - PML4[0..4]: Identity map first 4GB (for UEFI compatibility)
    /// - PML4[256]: Map first 4GB at 0xFFFF_8000_0000_0000 (kernel direct map)
    pub fn init_our_page_tables() {
        if INITIALIZED.swap(true, Ordering::SeqCst) {
            return;
        }
        
        // Allocate PML4 (use identity mapping since we're not yet initialized)
        let pml4_addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        unsafe { core::ptr::write_bytes(pml4_addr as *mut u8, 0, 4096); }
        OUR_PML4.store(pml4_addr, Ordering::SeqCst);
        
        let pml4 = pml4_addr as *mut u64;
        
        unsafe {
            // Allocate PDPT for first 4GB
            let pdpt_addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
            core::ptr::write_bytes(pdpt_addr as *mut u8, 0, 4096);
            let pdpt = pdpt_addr as *mut u64;
            
            // Create 4 PDs for 4GB of mapping using 2MB huge pages
            for gb in 0..4u64 {
                let pd_addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
                core::ptr::write_bytes(pd_addr as *mut u8, 0, 4096);
                *pdpt.add(gb as usize) = pd_addr | 0x3; // PRESENT | WRITABLE
                
                let pd = pd_addr as *mut u64;
                
                // Fill PD with 512 x 2MB huge pages
                for i in 0..512u64 {
                    let phys_addr = (gb << 30) | (i << 21);
                    // 2MB huge page: PRESENT | WRITABLE | HUGE_PAGE
                    *pd.add(i as usize) = phys_addr | 0x83;
                }
            }
            
            // === IDENTITY MAPPING: PML4[0] -> PDPT ===
            // Maps 0x00000000 - 0xFFFFFFFF to itself
            *pml4.add(0) = pdpt_addr | 0x3;
            
            // === KERNEL DIRECT MAP: PML4[256] -> same PDPT ===
            // Maps 0xFFFF_8000_0000_0000 - 0xFFFF_8000_FFFF_FFFF to physical 0x0 - 0xFFFFFFFF
            // PML4 index 256 = (0xFFFF_8000_0000_0000 >> 39) & 0x1FF = 256
            *pml4.add(256) = pdpt_addr | 0x3;
            
            log::info!("[Paging] Created page tables at 0x{:x}", pml4_addr);
            log::info!("[Paging] Identity mapping: 0x0 - 0xFFFFFFFF");
            log::info!("[Paging] Direct map: 0xFFFF_8000_0000_0000 + phys");
            
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
    
    /// Map a 4KB page for user access using offset mapping navigation
    unsafe fn map_page_user_4k(vaddr: u64) {
        let pml4_phys = OUR_PML4.load(Ordering::SeqCst);
        let pml4 = phys_to_virt(pml4_phys) as *mut u64;
        
        let pml4_idx = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt_idx = ((vaddr >> 30) & 0x1FF) as usize;
        let pd_idx = ((vaddr >> 21) & 0x1FF) as usize;
        let pt_idx = ((vaddr >> 12) & 0x1FF) as usize;
        
        // Navigate PML4
        let mut pml4_entry = *pml4.add(pml4_idx);
        if pml4_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pml4.add(pml4_idx) = frame | 0x7; // PRESENT | WRITABLE | USER
            pml4_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pml4.add(pml4_idx) |= 0x7; // Ensure USER bit
        
        // Navigate PDPT
        let pdpt_phys = pml4_entry & !0xFFF;
        let pdpt = phys_to_virt(pdpt_phys) as *mut u64;
        let mut pdpt_entry = *pdpt.add(pdpt_idx);
        
        // Check for 1GB huge page
        if pdpt_entry & 0x80 != 0 {
            *pdpt.add(pdpt_idx) |= 0x4; // Add USER
            return;
        }
        
        if pdpt_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pdpt.add(pdpt_idx) = frame | 0x7;
            pdpt_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pdpt.add(pdpt_idx) |= 0x7;
        
        // Navigate PD
        let pd_phys = pdpt_entry & !0xFFF;
        let pd = phys_to_virt(pd_phys) as *mut u64;
        let mut pd_entry = *pd.add(pd_idx);
        
        // Check for 2MB huge page
        if pd_entry & 0x80 != 0 {
            *pd.add(pd_idx) |= 0x4; // Add USER
            return;
        }
        
        if pd_entry & 1 == 0 {
            let frame = alloc_pt_page();
            *pd.add(pd_idx) = frame | 0x7;
            pd_entry = frame | 0x7;
            x86_64::instructions::tlb::flush_all();
        }
        *pd.add(pd_idx) |= 0x7;
        
        // Navigate PT
        let pt_phys = pd_entry & !0xFFF;
        let pt = phys_to_virt(pt_phys) as *mut u64;
        
        if *pt.add(pt_idx) & 1 == 0 {
            let frame = alloc_user_page();
            *pt.add(pt_idx) = frame | 0x7;
        } else {
            *pt.add(pt_idx) |= 0x4; // Add USER
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
