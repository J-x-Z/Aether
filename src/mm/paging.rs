//! Paging Support
//! 
//! Platform-specific paging implementations

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use x86_64::structures::paging::{
        PageTable, OffsetPageTable, Page, PhysFrame, Mapper, FrameAllocator, Size4KiB, PageTableFlags
    };
    use x86_64::{PhysAddr, VirtAddr};
    use spin::Mutex;
    use core::sync::atomic::{AtomicU64, Ordering};
    
    /// Simple bump allocator for physical frames
    /// Starts at 16MB to avoid UEFI reserved memory
    static NEXT_FRAME: AtomicU64 = AtomicU64::new(0x1000000); // 16MB
    
    struct SimpleFrameAllocator;
    
    unsafe impl FrameAllocator<Size4KiB> for SimpleFrameAllocator {
        fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
            let frame_addr = NEXT_FRAME.fetch_add(4096, Ordering::SeqCst);
            Some(PhysFrame::containing_address(PhysAddr::new(frame_addr)))
        }
    }
    
    /// Initialize and return the active page table mapper
    /// unsafe: Assumes identity mapping (offset 0)
    pub unsafe fn active_mapper() -> OffsetPageTable<'static> {
        let phys_mem_offset = VirtAddr::new(0);
        let level_4_table_ptr = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        let level_4_table = &mut *(level_4_table_ptr as *mut PageTable);
        OffsetPageTable::new(level_4_table, phys_mem_offset)
    }
    
    /// Ensure a range of addresses is accessible to User Mode (Ring 3)
    /// This will:
    /// 1. Allocate physical frames for unmapped pages
    /// 2. Map them with USER_ACCESSIBLE flag
    /// 3. Update existing mappings to add USER_ACCESSIBLE flag
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        let mut mapper = unsafe { active_mapper() };
        let mut allocator = SimpleFrameAllocator;
        
        let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(start_addr));
        let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(start_addr + len.saturating_sub(1)));
        
        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::USER_ACCESSIBLE;
        
        for page in Page::range_inclusive(start_page, end_page) {
            use x86_64::structures::paging::mapper::{Translate, TranslateResult};
            
            match mapper.translate(page.start_address()) {
                TranslateResult::Mapped { frame, flags: existing_flags, .. } => {
                    // Page already mapped, add USER_ACCESSIBLE flag
                    let new_flags = existing_flags | PageTableFlags::USER_ACCESSIBLE;
                    unsafe {
                        if let Ok(flush) = mapper.update_flags(page, new_flags) {
                            flush.flush();
                        }
                    }
                },
                _ => {
                    // Page not mapped, allocate and map it
                    unsafe {
                        match mapper.map_to(page, allocator.allocate_frame().unwrap(), flags, &mut allocator) {
                            Ok(flush) => flush.flush(),
                            Err(e) => {
                                log::warn!("[Paging] Failed to map {:?}: {:?}", page, e);
                            }
                        }
                    }
                }
            }
        }
        
        // IMPORTANT: Also ensure all parent page table entries have USER bit set
        // This is critical because x86_64 requires USER bit at ALL levels
        unsafe {
            propagate_user_bit(start_addr, len);
        }
    }
    
    /// Propagate USER bit to all parent page table entries
    /// x86_64 requires USER bit at PML4, PDPT, PD, and PT levels
    unsafe fn propagate_user_bit(start_addr: u64, len: u64) {
        let cr3 = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        let pml4 = &mut *(cr3 as *mut PageTable);
        
        let start = start_addr;
        let end = start_addr + len;
        
        // For each address in range, walk the page table and set USER bits
        let mut addr = start & !0xFFF; // Page align
        while addr < end {
            let pml4_idx = ((addr >> 39) & 0x1FF) as usize;
            let pdpt_idx = ((addr >> 30) & 0x1FF) as usize;
            let pd_idx = ((addr >> 21) & 0x1FF) as usize;
            let pt_idx = ((addr >> 12) & 0x1FF) as usize;
            
            // Set USER on PML4 entry
            if pml4[pml4_idx].flags().contains(PageTableFlags::PRESENT) {
                let flags = pml4[pml4_idx].flags() | PageTableFlags::USER_ACCESSIBLE;
                pml4[pml4_idx].set_flags(flags);
                
                // Get PDPT
                let pdpt_addr = pml4[pml4_idx].addr().as_u64();
                let pdpt = &mut *(pdpt_addr as *mut PageTable);
                
                if pdpt[pdpt_idx].flags().contains(PageTableFlags::PRESENT) {
                    let flags = pdpt[pdpt_idx].flags() | PageTableFlags::USER_ACCESSIBLE;
                    pdpt[pdpt_idx].set_flags(flags);
                    
                    // Check if this is a 1GB page
                    if !pdpt[pdpt_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
                        // Get PD
                        let pd_addr = pdpt[pdpt_idx].addr().as_u64();
                        let pd = &mut *(pd_addr as *mut PageTable);
                        
                        if pd[pd_idx].flags().contains(PageTableFlags::PRESENT) {
                            let flags = pd[pd_idx].flags() | PageTableFlags::USER_ACCESSIBLE;
                            pd[pd_idx].set_flags(flags);
                            
                            // Check if this is a 2MB page
                            if !pd[pd_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
                                // Get PT
                                let pt_addr = pd[pd_idx].addr().as_u64();
                                let pt = &mut *(pt_addr as *mut PageTable);
                                
                                if pt[pt_idx].flags().contains(PageTableFlags::PRESENT) {
                                    let flags = pt[pt_idx].flags() | PageTableFlags::USER_ACCESSIBLE;
                                    pt[pt_idx].set_flags(flags);
                                }
                            }
                        }
                    }
                }
            }
            
            addr += 4096;
        }
        
        // Flush TLB
        x86_64::instructions::tlb::flush_all();
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
