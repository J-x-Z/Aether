//! ARM64 Memory Management Unit (MMU)
//!
//! Handles ARM64 page table format (4KB granule, 4-level translation)
//!
//! ARM64 Translation Table Format (4KB granule):
//! - Level 0-3 page tables
//! - Each entry is 8 bytes (64-bit)
//! - 512 entries per table (4KB / 8 = 512)
//!
//! Virtual Address Layout (48-bit):
//! [63:48] - Sign extension (must match bit 47 or fault)
//! [47:39] - L0 index (9 bits)
//! [38:30] - L1 index (9 bits)
//! [29:21] - L2 index (9 bits)
//! [20:12] - L3 index (9 bits)
//! [11:0]  - Page offset (12 bits = 4KB)

use core::arch::asm;

/// Page table entry flags (lower attributes)
pub mod flags {
    /// Entry is valid/present
    pub const VALID: u64 = 1 << 0;
    
    /// Entry is a table descriptor (not block)
    pub const TABLE: u64 = 1 << 1;
    
    /// For L3: Page descriptor type
    pub const PAGE: u64 = 1 << 1;
    
    /// Access Permission [7:6]
    /// AP[2:1] = 01: EL1 RW, EL0 RW
    pub const AP_RW_EL1_RW_EL0: u64 = 0b01 << 6;
    
    /// AP[2:1] = 00: EL1 RW, EL0 no access  
    pub const AP_RW_EL1: u64 = 0b00 << 6;
    
    /// AP[2:1] = 11: EL1 RO, EL0 RO
    pub const AP_RO_ALL: u64 = 0b11 << 6;
    
    /// Non-shareable
    pub const SH_NON: u64 = 0b00 << 8;
    
    /// Outer shareable
    pub const SH_OUTER: u64 = 0b10 << 8;
    
    /// Inner shareable
    pub const SH_INNER: u64 = 0b11 << 8;
    
    /// Access flag (must be set or takes access fault)
    pub const AF: u64 = 1 << 10;
    
    /// User Execute Never (clear to allow EL0 execution)
    pub const UXN: u64 = 1 << 54;
    
    /// Privileged Execute Never
    pub const PXN: u64 = 1 << 53;
}

/// Read TTBR0_EL1 (Translation Table Base Register for EL0)
pub fn read_ttbr0() -> u64 {
    let val: u64;
    unsafe {
        asm!("mrs {}, ttbr0_el1", out(reg) val);
    }
    val
}

/// Read TTBR1_EL1 (Translation Table Base Register for EL1)
pub fn read_ttbr1() -> u64 {
    let val: u64;
    unsafe {
        asm!("mrs {}, ttbr1_el1", out(reg) val);
    }
    val
}

/// Invalidate TLB for a specific address
pub fn tlb_invalidate_page(vaddr: u64) {
    unsafe {
        asm!(
            "dsb ishst",      // Data Sync Barrier (inner shareable, store)
            "tlbi vaae1is, {addr}",  // TLB Invalidate by VA, All ASIDs, EL1, Inner Shareable
            "dsb ish",        // Data Sync Barrier
            "isb",            // Instruction Sync Barrier
            addr = in(reg) vaddr >> 12,  // Shift to get page frame number
        );
    }
}

/// Invalidate entire TLB
pub fn tlb_invalidate_all() {
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vmalle1is",  // TLB Invalidate by VMID, All at stage 1, EL1, Inner Shareable
            "dsb ish",
            "isb",
        );
    }
}

/// Walk page tables and set user access flags
/// 
/// This function walks the ARM64 4-level page table starting from TTBR0_EL1
/// and sets AP bits to allow EL0 (user) access.
pub fn make_user_accessible(start_addr: u64, len: u64) {
    log::info!(
        "[MMU] ARM64: Marking 0x{:x}-0x{:x} as user accessible ({} bytes)",
        start_addr,
        start_addr + len,
        len
    );
    
    // Get the base of the page table hierarchy from TTBR0_EL1
    let ttbr0 = read_ttbr0();
    let l0_table_phys = ttbr0 & 0xFFFF_FFFF_F000; // Mask to get physical address (remove ASID)
    
    log::debug!("[MMU] TTBR0_EL1 = 0x{:x}, L0 table @ 0x{:x}", ttbr0, l0_table_phys);
    
    // For UEFI identity mapping, virt == phys
    let l0_table = l0_table_phys as *mut u64;
    
    // Process each page in the range
    let page_size = 4096u64;
    let mut addr = start_addr & !(page_size - 1);  // Align to page boundary
    let end = (start_addr + len + page_size - 1) & !(page_size - 1);
    
    while addr < end {
        // Calculate indices for each level
        let l0_idx = ((addr >> 39) & 0x1FF) as usize;
        let l1_idx = ((addr >> 30) & 0x1FF) as usize;
        let l2_idx = ((addr >> 21) & 0x1FF) as usize;
        let l3_idx = ((addr >> 12) & 0x1FF) as usize;
        
        unsafe {
            // Walk L0 -> L1
            let l0_entry = *l0_table.add(l0_idx);
            if (l0_entry & flags::VALID) == 0 {
                log::warn!("[MMU] L0[{}] not valid for addr 0x{:x}", l0_idx, addr);
                addr += page_size;
                continue;
            }
            
            let l1_table = (l0_entry & 0xFFFF_FFFF_F000) as *mut u64;
            let l1_entry = *l1_table.add(l1_idx);
            if (l1_entry & flags::VALID) == 0 {
                log::warn!("[MMU] L1[{}] not valid for addr 0x{:x}", l1_idx, addr);
                addr += page_size;
                continue;
            }
            
            // Check if L1 is a 1GB block (not a table)
            if (l1_entry & flags::TABLE) == 0 {
                // It's a 1GB block - modify in place
                let new_entry = l1_entry | flags::AP_RW_EL1_RW_EL0 | flags::AF;
                *l1_table.add(l1_idx) = new_entry;
                tlb_invalidate_page(addr);
                addr += 0x4000_0000; // 1GB
                continue;
            }
            
            let l2_table = (l1_entry & 0xFFFF_FFFF_F000) as *mut u64;
            let l2_entry = *l2_table.add(l2_idx);
            if (l2_entry & flags::VALID) == 0 {
                log::warn!("[MMU] L2[{}] not valid for addr 0x{:x}", l2_idx, addr);
                addr += page_size;
                continue;
            }
            
            // Check if L2 is a 2MB block
            if (l2_entry & flags::TABLE) == 0 {
                // It's a 2MB block - modify in place
                let new_entry = l2_entry | flags::AP_RW_EL1_RW_EL0 | flags::AF;
                *l2_table.add(l2_idx) = new_entry;
                tlb_invalidate_page(addr);
                addr += 0x20_0000; // 2MB
                continue;
            }
            
            let l3_table = (l2_entry & 0xFFFF_FFFF_F000) as *mut u64;
            let l3_entry = *l3_table.add(l3_idx);
            if (l3_entry & flags::VALID) == 0 {
                log::warn!("[MMU] L3[{}] not valid for addr 0x{:x}", l3_idx, addr);
                addr += page_size;
                continue;
            }
            
            // Modify L3 entry (4KB page)
            // Set AP[2:1] = 01 (EL1 RW, EL0 RW) and clear UXN (allow user execute)
            let mut new_entry = l3_entry;
            new_entry &= !(0b11 << 6);            // Clear AP bits
            new_entry |= flags::AP_RW_EL1_RW_EL0; // Set RW for both EL1 and EL0
            new_entry &= !flags::UXN;             // Clear UXN to allow user execution
            new_entry |= flags::AF;               // Ensure AF is set
            
            *l3_table.add(l3_idx) = new_entry;
            tlb_invalidate_page(addr);
        }
        
        addr += page_size;
    }
    
    log::info!("[MMU] ARM64: User access configured for 0x{:x}-0x{:x}", start_addr, start_addr + len);
}

/// Initialize MMU for ARM64
pub fn init() {
    log::info!("[MMU] ARM64 MMU initialized");
    log::info!("[MMU] TTBR0_EL1 = 0x{:x} (user space)", read_ttbr0());
    log::info!("[MMU] TTBR1_EL1 = 0x{:x} (kernel space)", read_ttbr1());
    // UEFI sets up identity mapping, we use it as-is for now
}
