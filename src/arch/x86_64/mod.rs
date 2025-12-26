//! Architecture-specific code for x86_64

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod syscall;

/// Initialize x86_64 architecture
pub fn init() {
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
    let rflags = 0x202; 
    
    // IRETQ Stack Frame: SS, RSP, RFLAGS, CS, RIP
    core::arch::asm!(
        "mov ds, {ds:x}",
        "mov es, {ds:x}",
        "mov fs, {ds:x}",
        "mov gs, {ds:x}",
        
        "push {ss}",  // SS
        "push {rsp}", // RSP
        "push {rflags}", // RFLAGS
        "push {cs}",  // CS
        "push {rip}", // RIP
        "iretq",
        ds = in(reg) user_ds,
        ss = in(reg) user_ds as u64, // Pushed as u64
        rsp = in(reg) stack_pointer,
        rflags = in(reg) rflags,
        cs = in(reg) user_cs as u64,
        rip = in(reg) entry_point,
        options(noreturn)
    );
}
