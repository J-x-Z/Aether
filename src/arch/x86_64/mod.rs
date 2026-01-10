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
}

/// Jump to userspace (Ring 3)
/// Does not return.
pub unsafe fn enter_usermode(entry_point: u64, stack_pointer: u64) -> ! {
    let user_cs = gdt::user_cs();
    let user_ds = gdt::user_ds();
    
    // RFLAGS: Interrupts enabled (bit 9), Reserved (bit 1) should be 1
    let rflags: u64 = 0x202; 
    
    // IRETQ Stack Frame: SS, RSP, RFLAGS, CS, RIP
    core::arch::asm!(
        "cli", // Disable interrupts while setting up segments
        
        // Load NULL into data segment registers (safe in Ring0)
        "xor ax, ax",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        
        "push {ss}",  // SS (user data selector)
        "push {rsp}", // RSP
        "push {rflags}", // RFLAGS
        "push {cs}",  // CS (user code selector)
        "push {rip}", // RIP
        
        "iretq",
        ss = in(reg) user_ds as u64, // Pushed as u64
        rsp = in(reg) stack_pointer,
        rflags = in(reg) rflags,
        cs = in(reg) user_cs as u64,
        rip = in(reg) entry_point,
        options(noreturn)
    );
}

