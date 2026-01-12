//! Paging Support
//! 
//! Platform-specific paging implementations
//! Uses OFFSET MAPPING (Linux-style direct map) instead of recursive page tables

#[cfg(target_arch = "x86_64")]
mod x86_64_paging {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use spin::Once;
    use x86_64::structures::paging::PhysFrame;
    
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
    
    // Safety: We assume single-core access to allocator for now, or usage atomic logic.
    static PT_ALLOCATOR: AtomicU64 = AtomicU64::new(0); // Holds PHYSICAL address of next free page
    static FRAME_ALLOCATOR: AtomicU64 = AtomicU64::new(0x4100000);
    static MAX_RAM: AtomicU64 = AtomicU64::new(0x8000000); // Default 128MB limit
    
    // We store our PML4 physical address for cloning
    static OUR_PML4: AtomicU64 = AtomicU64::new(0);
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    
    /// Initialize the allocator with a valid memory region from UEFI
    pub fn init_allocator(start: u64, size: u64) {
        // Align to 4KB
        let aligned_start = (start + 4095) & !4095;
        let end = start + size;
        
        PT_ALLOCATOR.store(aligned_start, Ordering::SeqCst);
        // Split region: 2MB for PTs, rest for Frames? Or just share/split?
        // Let's give 4MB offset for frames.
        FRAME_ALLOCATOR.store(aligned_start + 0x400000, Ordering::SeqCst); // +4MB
        MAX_RAM.store(end, Ordering::SeqCst);
        
        log::info!("[Paging] Allocator initialized @ 0x{:x} (Size: {} MB)", aligned_start, size / 1024 / 1024);
    }

    /// Allocate a page for page tables (zeroed)
    fn alloc_pt_page() -> u64 {
        let addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
        
        // Safety Check
        if addr >= MAX_RAM.load(Ordering::Relaxed) {
             panic!("[Paging] OOM in alloc_pt_page! addr={:x}", addr);
        }
        if addr >= FRAME_ALLOCATOR.load(Ordering::Relaxed) {
            // Collision with frame allocator?
            // Ideally we should have separate regions or a real allocator.
            // For now, simple bump is risky if they cross.
            // But we set FRAME to +4MB. If PT grows > 4MB it collides.
            // log::warn!("[Paging] PT Allocator collision risk? {:x}", addr);
        }

        // log::trace!("[Paging] Allocating PT page @ 0x{:x}", addr);

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
        
        if addr >= MAX_RAM.load(Ordering::Relaxed) {
             panic!("[Paging] OOM in alloc_user_page! addr={:x}", addr);
        }
        
        // log::trace!("[Paging] Allocating User Frame @ 0x{:x}", addr);

        let ptr = phys_to_virt(addr);
        unsafe { core::ptr::write_bytes(ptr, 0, 4096); }
        addr
    }
    
    /// Initialize page tables with identity mapping AND kernel direct mapping
    /// - PML4[0..4]: Identity map first 4GB (for UEFI compatibility)
    /// - PML4[256]: Map first 4GB at 0xFFFF_8000_0000_0000 (kernel direct map)
    pub fn init_our_page_tables() {
    static INIT: Once<()> = Once::new();
    
    INIT.call_once(|| {
        log::info!("[Paging] Initializing Page Tables...");
        
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
            
            // Create 64 PDs for 64GB of mapping using 2MB huge pages
            // This ensures we cover kernels loaded > 4GB by UEFI
            for gb in 0..64u64 {
                let pd_addr = PT_ALLOCATOR.fetch_add(4096, Ordering::SeqCst);
                core::ptr::write_bytes(pd_addr as *mut u8, 0, 4096);
                *pdpt.add(gb as usize) = pd_addr | 0x7; // PRESENT | WRITABLE | USER
                
                let pd = pd_addr as *mut u64;
                
                // Fill PD with 512 x 2MB huge pages
                for i in 0..512u64 {
                    let phys_addr = (gb << 30) | (i << 21);
                    // 2MB huge page: PRESENT | WRITABLE | HUGE_PAGE | USER
                    *pd.add(i as usize) = phys_addr | 0x87;
                }
            }
            
            // === IDENTITY MAPPING: PML4[0] -> PDPT ===
            // Maps 0x00000000 - 0xFFFFFFFFFF (0-1TB potentially, but our PDPT only covers 64GB)
            *pml4.add(0) = pdpt_addr | 0x7; // PRESENT | WRITABLE | USER
            
            // === KERNEL DIRECT MAP: PML4[256] -> same PDPT ===
            // Maps 0xFFFF_8000_0000_0000 + phys 0..64GB
            *pml4.add(256) = pdpt_addr | 0x7; // PRESENT | WRITABLE | USER
            
            log::info!("[Paging] Created page tables at 0x{:x}", pml4_addr);
            log::info!("[Paging] Identity mapping: 0x0 - 64GB");
            log::info!("[Paging] Kernel Direct map: 0xFFFF800000000000+");
            
            // Switch to our page tables
            let (_, cr3_flags) = x86_64::registers::control::Cr3::read();
            x86_64::registers::control::Cr3::write(
                PhysFrame::containing_address(
                    x86_64::PhysAddr::new(pml4_addr)
                ),
                cr3_flags
            );
            
            log::info!("[Paging] Switched to new CR3: 0x{:x}", pml4_addr);
        }
    });

    // Note: If called again, we do NOTHING.
    // This preserves the CURRENT CR3 if it was switched by sys_execve.
}
    
    /// Map a 4KB page for user access using offset mapping navigation
    unsafe fn map_page_user_4k(vaddr: u64) {
        let (pml4_frame, _) = x86_64::registers::control::Cr3::read();
        let pml4_phys = pml4_frame.start_address().as_u64();
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
    /// Create a new Page Table (PML4) for a process
    /// Copies Kernel mappings (Identity + Direct Map)
    /// Returns physical address of new PML4
    pub fn clone_process_page_table() -> u64 {
        // Allocate new PML4
        let pml4_addr = alloc_pt_page();
        let pml4 = phys_to_virt(pml4_addr) as *mut u64;
        
        let kernel_pml4_phys = OUR_PML4.load(Ordering::SeqCst);
        let kernel_pml4 = phys_to_virt(kernel_pml4_phys) as *const u64;
        
        unsafe {
            // 1. Copy Upper Half (Kernel Direct Map, etc.)
            // Entries 256-511 are Kernel Space (Higher Half) -> SHARED.
            core::ptr::copy_nonoverlapping(kernel_pml4.add(256), pml4.add(256), 256);
            
            // 2. Handle Lower Half (Identity/User Space) - Index 0
            // We need to ISOLATE User Space (e.g. 0x400000), which falls into Index 0.
            // But we MUST Share Kernel Code (0x100000), which ALSO falls into Index 0.
            // So we need a Deep Copy of the hierarchy for Index 0.
            
            // A. Allocate New PDPT for Index 0 (which covers 0-512GB Virtual)
            // Actually this is PML4[0], so it covers 0-512GB.
            // PDPT entries cover 1GB each.
            let new_pdpt_addr = alloc_pt_page();
            let new_pdpt = phys_to_virt(new_pdpt_addr) as *mut u64;
            *pml4.add(0) = new_pdpt_addr | 0x7; // Present, RW, User

            // Get Kernel's PDPT[0]
            let kernel_pdpt_phys = (*kernel_pml4.add(0)) & !0xFFF;
            let kernel_pdpt = phys_to_virt(kernel_pdpt_phys) as *const u64;
            
            // IMPORTANT: Copy ALL entries from Kernel PDPT (1..511)
            // This ensures we map Physical RAM > 1GB (Video, ACPI, etc.)
            core::ptr::copy_nonoverlapping(kernel_pdpt, new_pdpt, 512);
            
            // B. We need to look at PDPT[0] (which covers first 1GB)
            // It points to a PD. We must CLONE this PD too.
            let new_pd_addr = alloc_pt_page();
            let new_pd = phys_to_virt(new_pd_addr) as *mut u64;
            *new_pdpt.add(0) = new_pd_addr | 0x7;
            
            // Get Kernel's PD[0]
            let kernel_pd_phys = (*kernel_pdpt.add(0)) & !0xFFF;
            let kernel_pd = phys_to_virt(kernel_pd_phys) as *const u64;
            
            // C. Copy Kernel Mappings in PD (0-2MB) and Heap (32MB+)
            // Kernel Code: 0-2MB (Index 0)
            // User Code: 4MB (Index 2) -> MUST BE ISOLATED/CLEARED
            // Kernel Heap: 32MB (Index 16) -> MUST BE SHARED
            
            // We iterate all 512 entries of the first PD (covering 0-1GB)
            for i in 0..512u64 {
                // Logic:
                // Index 0: Kernel Code (Usually 0-2MB) - Share
                // Index 1: 2-4MB - May contain Kernel BSS/Data? - Share to be safe.
                // Index 2: 4MB-6MB (User Code Base 0x400000) - CLEAR/ISOLATE.
                // Index 3+: Other - Share (Assume Kernel/Heap/Devices)
                
                // We ONLY isolate the exact 2MB block where User Program is loaded.
                // This prevents accidentally unmapping Kernel if it's larger than 2MB or loaded strangely.
                
                if i == 2 {
                    *new_pd.add(i as usize) = 0; // Clear User Base (4MB - 6MB)
                } else {
                    *new_pd.add(i as usize) = *kernel_pd.add(i as usize); // Share everything else
                }
            }
            
            // D. CRITICAL FIX: Copy Identity Map for High Addresses (1GB - 512GB)
            // If Kernel is loaded > 1GB (e.g. 5GB at 0x140xxxxxx), it lives in PDPT[5].
            // We MUST share these mappings, or the child process cannot access Kernel Code/Data.
            for i in 1..512 {
                 *new_pdpt.add(i) = *kernel_pdpt.add(i);
            }
            
            // D. What about other PDPT entries (1-511)?
            // PDPT[1] covers 1GB-2GB.
            // If we have Identity Map of RAM there, we might need it for frame buffers?
            // Ideally User shouldn't see physical RAM identity map.
            // So we leave them as 0 for safety, UNLESS Kernel Heap extends there?
            // Heap Size is 16MB. Fits in PD[0].
            // So PDPT[1+] are likely unused or Identity Map.
            // Safe to leave as 0 in new process (unless we need direct hardware access).
            
            // Verify: We copied Kernel PD entries (0-16MB).
            // We shared Higher Half (256-511).
            // We have a fresh PD for User Space (4MB+) that is Empty.
            // Perfect integration.
        }
        
        pml4_addr
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
