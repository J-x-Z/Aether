//! Architecture-specific code for x86_64

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod syscall;

/// Initialize x86_64 architecture
pub fn init() {
    // CRITICAL: Disable ALL interrupts before touching GDT.
    // UEFI has its own IDT and timer running. If an interrupt fires
    // after we load our GDT but before our IDT is ready, the CPU will
    // use UEFI's IDT entries which reference UEFI's old GDT selectors
    // (like CS=0x38) that no longer exist in our GDT -> GPF -> Triple Fault.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)); }
    
    gdt::init();
    // interrupts::init_idt(); // Moved to main.rs for now or here
    syscall::init();
    
    // Enable SSE (Required for userspace SIMD)
    enable_sse();
}

fn enable_sse() {
    log::info!("[Arch] Enabling SSE...");
    
    // 1. Enable SSE in CR0 (Clear EM, Set MP)
    // CR0.EM (Bit 2) = 0
    // CR0.MP (Bit 1) = 1
    unsafe {
        let mut cr0: u64;
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack));
        
        // Clear EM (bit 2)
        cr0 &= !(1 << 2);
        // Set MP (bit 1)
        cr0 |= (1 << 1);
        
        core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack));
    }
    
    // 2. Enable SSE in CR4 (Set OSFXSR, OSXMMEXCPT)
    // CR4.OSFXSR (Bit 9) = 1
    // CR4.OSXMMEXCPT (Bit 10) = 1
    unsafe {
        let mut cr4: u64;
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
        
        // Set OSFXSR (bit 9) and OSXMMEXCPT (bit 10)
        cr4 |= (1 << 9) | (1 << 10);
        
        core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack));
    }
    
    // 3. Initialize FPU/SSE state (important for musl startup)
    unsafe {
        core::arch::asm!("fninit", options(nomem, nostack));
    }
}

/// Jump to userspace (Ring 3)
/// Does not return.
pub unsafe fn enter_usermode(entry_point: u64, stack_pointer: u64) -> ! {
    let user_cs = gdt::user_cs();
    let user_ds = gdt::user_ds();
    
    // RFLAGS: Interrupts enabled (bit 9), Reserved (bit 1) should be 1
    let rflags = 0x202; 
    
    // Verify IRETQ arguments
    // LOAD-BEARING DEBUG PRINTS: Removing this causes GPK0 (IRETQ Failure)
    crate::video::console_print_char(b'I');
    crate::video::console_print_char(b':');
    
    // Print CS (User Code)
    let cs_val = user_cs;
    crate::video::console_print_char(if (cs_val >> 4) < 10 { b'0' + (cs_val >> 4) as u8 } else { b'a' + (cs_val >> 4) as u8 - 10 });
    crate::video::console_print_char(if (cs_val & 0xF) < 10 { b'0' + (cs_val & 0xF) as u8 } else { b'a' + (cs_val & 0xF) as u8 - 10 });
    
    // Print SS (User Data)
    crate::video::console_print_char(b'/');
    let ss_val = user_ds;
    crate::video::console_print_char(if (ss_val >> 4) < 10 { b'0' + (ss_val >> 4) as u8 } else { b'a' + (ss_val >> 4) as u8 - 10 });
    crate::video::console_print_char(if (ss_val & 0xF) < 10 { b'0' + (ss_val & 0xF) as u8 } else { b'a' + (ss_val & 0xF) as u8 - 10 });
    
    // Print RSP (High Nibbles)
    crate::video::console_print_char(b'/');
    let sp_high = ((stack_pointer >> 28) & 0xF) as u8;
    crate::video::console_print_char(if sp_high < 10 { b'0' + sp_high } else { b'a' + sp_high - 10 });

    // Print RIP (First Byte) - reused from earlier
    crate::video::console_print_char(b'/');
    let syscall_first_byte = unsafe { *(entry_point as *const u8) };
    let sfb_high = (syscall_first_byte >> 4) & 0xF;
    crate::video::console_print_char(if sfb_high < 10 { b'0' + sfb_high } else { b'a' + sfb_high - 10 });
    
    // DEBUG: Output 'U' to screen before entering usermode
    crate::video::console_print_char(b'U');
    
    // Clean entry to User Mode (clearing MSRs to avoid garbage from UEFI)
    core::arch::asm!(
        "cli", 
        "xor eax, eax",
        "xor edx, edx",
        "mov ecx, 0xC0000100", // FS.base
        "wrmsr",
        "mov ecx, 0xC0000101", // GS.base
        "wrmsr",
        "mov ecx, 0xC0000102", // KernelGSbase
        "wrmsr",
        
        "mov ds, {ds:x}",
        "mov es, {ds:x}",
        "mov fs, {ds:x}",
        "mov gs, {ds:x}",
        
        "push {ss}", 
        "push {rsp}", 
        "push {rflags}", 
        "push {cs}", 
        "push {rip}", 
        "sti",
        "iretq",
        
        ds = in(reg) user_ds,
        ss = in(reg) user_ds as u64,
        rsp = in(reg) stack_pointer,
        rflags = in(reg) rflags,
        cs = in(reg) user_cs as u64,
        rip = in(reg) entry_point,
        options(noreturn)
    );
}
