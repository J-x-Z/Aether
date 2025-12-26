//! ARM64 (AArch64) Architecture Module

pub mod exception;
pub mod svc;
pub mod mmu;

use spin::Lazy;

/// Initialize ARM64 architecture
pub fn init() {
    log::info!("[Arch] Initializing ARM64 (AArch64)...");
    exception::init();
    svc::init();
    log::info!("[Arch] ARM64 initialization complete");
}

/// Enter usermode (EL0) from kernel (EL1)
/// 
/// This function sets up SPSR_EL1 and ELR_EL1 to return to EL0,
/// then executes `eret` to jump to userspace.
/// 
/// # Safety
/// - `entry_point` must point to valid userspace code
/// - `stack_pointer` must point to valid userspace stack
pub unsafe fn enter_usermode(entry_point: u64, stack_pointer: u64) -> ! {
    // SPSR_EL1 value for returning to EL0:
    // - M[3:0] = 0b0000 (EL0t - EL0 with SP_EL0)
    // - All interrupt masks clear (enable interrupts in userspace)
    // - NZCV flags = 0
    let spsr_el1: u64 = 0b0000; // EL0t
    
    core::arch::asm!(
        // Set stack pointer for EL0
        "msr sp_el0, {sp}",
        
        // Set return address (entry point)
        "msr elr_el1, {entry}",
        
        // Set saved program status (return to EL0)
        "msr spsr_el1, {spsr}",
        
        // Return to EL0
        "eret",
        
        sp = in(reg) stack_pointer,
        entry = in(reg) entry_point,
        spsr = in(reg) spsr_el1,
        options(noreturn)
    );
}

/// Get user code segment selector (for compatibility with x86 API)
pub fn user_cs() -> u16 {
    0 // ARM64 doesn't use segment selectors
}

/// Get user data segment selector (for compatibility with x86 API)
pub fn user_ds() -> u16 {
    0 // ARM64 doesn't use segment selectors
}

/// Get kernel code segment selector (for compatibility with x86 API)
pub fn kernel_cs() -> u16 {
    0 // ARM64 doesn't use segment selectors
}
