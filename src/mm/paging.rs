//! Paging Support
//! 
//! Platform-specific paging implementations

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use x86_64::structures::paging::{
        PageTable, OffsetPageTable, Page, PhysFrame, Mapper, FrameAllocator, Size4KiB, PageTableFlags
    };
    use x86_64::{PhysAddr, VirtAddr};
    
    /// Initialize and return the active page table mapper
    /// unsafe: Assumes identity mapping (offset 0)
    pub unsafe fn active_mapper() -> OffsetPageTable<'static> {
        let phys_mem_offset = VirtAddr::new(0);
        let level_4_table_ptr = x86_64::registers::control::Cr3::read().0.start_address().as_u64();
        let level_4_table = &mut *(level_4_table_ptr as *mut PageTable);
        OffsetPageTable::new(level_4_table, phys_mem_offset)
    }
    
    /// Ensure a range of addresses is accessible to User Mode (Ring 3)
    pub fn make_user_accessible(start_addr: u64, len: u64) {
        let mut mapper = unsafe { active_mapper() };
        
        let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(start_addr));
        let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(start_addr + len));
        
        for page in Page::range_inclusive(start_page, end_page) {
            use x86_64::structures::paging::mapper::{Translate, TranslateResult};
            match mapper.translate(page.start_address()) {
                 TranslateResult::Mapped { flags, .. } => {
                     let new_flags = flags | PageTableFlags::USER_ACCESSIBLE;
                     unsafe {
                         if let Ok(flush) = mapper.update_flags(page, new_flags) {
                             flush.flush();
                         }
                     }
                 },
                 _ => {}
            }
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
