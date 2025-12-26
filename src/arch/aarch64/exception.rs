//! ARM64 Exception Handling
//!
//! Sets up VBAR_EL1 (Vector Base Address Register) and exception vector table.

use core::arch::asm;

/// Exception Vector Table
/// ARM64 has 16 exception vectors (4 types x 4 exception levels)
#[repr(C, align(2048))]
struct ExceptionVectorTable {
    // Current EL with SP_EL0
    sync_current_el_sp0: [u8; 0x80],
    irq_current_el_sp0: [u8; 0x80],
    fiq_current_el_sp0: [u8; 0x80],
    serror_current_el_sp0: [u8; 0x80],
    
    // Current EL with SP_ELx
    sync_current_el_spx: [u8; 0x80],
    irq_current_el_spx: [u8; 0x80],
    fiq_current_el_spx: [u8; 0x80],
    serror_current_el_spx: [u8; 0x80],
    
    // Lower EL using AArch64
    sync_lower_el_aarch64: [u8; 0x80],
    irq_lower_el_aarch64: [u8; 0x80],
    fiq_lower_el_aarch64: [u8; 0x80],
    serror_lower_el_aarch64: [u8; 0x80],
    
    // Lower EL using AArch32
    sync_lower_el_aarch32: [u8; 0x80],
    irq_lower_el_aarch32: [u8; 0x80],
    fiq_lower_el_aarch32: [u8; 0x80],
    serror_lower_el_aarch32: [u8; 0x80],
}

/// Initialize exception handling
pub fn init() {
    log::info!("[Exception] Setting up ARM64 exception vectors...");
    
    unsafe {
        // Set VBAR_EL1 to point to our exception vector table
        let vbar = exception_vector_table as *const () as u64;
        asm!(
            "msr vbar_el1, {vbar}",
            vbar = in(reg) vbar,
            options(nostack, nomem)
        );
    }
    
    log::info!("[Exception] VBAR_EL1 configured");
}

/// Exception vector table (assembly implementation)
/// Each vector entry has 32 instructions (0x80 bytes)
/// Alignment to 2048 bytes is achieved via .balign directive
#[unsafe(naked)]
#[no_mangle]
#[link_section = ".text.vectors"]
unsafe extern "C" fn exception_vector_table() {
    core::arch::naked_asm!(
        // ========================================
        // Current EL with SP_EL0
        // ========================================
        
        // Synchronous - Current EL SP0
        "b sync_exception_handler",
        ".balign 0x80",
        
        // IRQ - Current EL SP0
        "b irq_handler",
        ".balign 0x80",
        
        // FIQ - Current EL SP0
        "b fiq_handler",
        ".balign 0x80",
        
        // SError - Current EL SP0
        "b serror_handler",
        ".balign 0x80",
        
        // ========================================
        // Current EL with SP_ELx
        // ========================================
        
        // Synchronous - Current EL SPx
        "b sync_exception_handler",
        ".balign 0x80",
        
        // IRQ - Current EL SPx
        "b irq_handler",
        ".balign 0x80",
        
        // FIQ - Current EL SPx
        "b fiq_handler",
        ".balign 0x80",
        
        // SError - Current EL SPx
        "b serror_handler",
        ".balign 0x80",
        
        // ========================================
        // Lower EL using AArch64
        // ========================================
        
        // Synchronous - Lower EL AArch64 (SVC from userspace)
        "b sync_lower_el_handler",
        ".balign 0x80",
        
        // IRQ - Lower EL AArch64
        "b irq_handler",
        ".balign 0x80",
        
        // FIQ - Lower EL AArch64
        "b fiq_handler",
        ".balign 0x80",
        
        // SError - Lower EL AArch64
        "b serror_handler",
        ".balign 0x80",
        
        // ========================================
        // Lower EL using AArch32 (not used)
        // ========================================
        
        "b unhandled_exception",
        ".balign 0x80",
        "b unhandled_exception",
        ".balign 0x80",
        "b unhandled_exception",
        ".balign 0x80",
        "b unhandled_exception",
        ".balign 0x80",
    );
}

/// Synchronous exception handler (kernel mode)
#[no_mangle]
extern "C" fn sync_exception_handler() {
    log::error!("[Exception] Synchronous exception in kernel mode!");
    loop { core::hint::spin_loop(); }
}

/// Synchronous exception from lower EL (userspace syscall)
#[no_mangle]
extern "C" fn sync_lower_el_handler() {
    // This is called when userspace executes SVC
    // Dispatch to syscall handler
    unsafe {
        let esr_el1: u64;
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr_el1);
        
        let ec = (esr_el1 >> 26) & 0x3F;
        
        if ec == 0x15 {
            // SVC from AArch64 (syscall)
            crate::arch::aarch64::svc::handle_svc();
        } else {
            log::error!("[Exception] Unhandled exception from EL0: EC=0x{:x}", ec);
        }
    }
}

/// IRQ handler
#[no_mangle]
extern "C" fn irq_handler() {
    log::info!("[IRQ] Interrupt received");
    // TODO: Handle interrupts
}

/// FIQ handler
#[no_mangle]
extern "C" fn fiq_handler() {
    log::warn!("[FIQ] Fast interrupt received");
}

/// SError handler  
#[no_mangle]
extern "C" fn serror_handler() {
    log::error!("[SError] System error!");
    loop { core::hint::spin_loop(); }
}

/// Unhandled exception
#[no_mangle]
extern "C" fn unhandled_exception() {
    log::error!("[Exception] Unhandled!");
    loop { core::hint::spin_loop(); }
}
