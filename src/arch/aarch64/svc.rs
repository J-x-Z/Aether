//! ARM64 SVC (Supervisor Call) Handler
//!
//! Handles syscalls from userspace (EL0) via the SVC instruction.
//! ARM64 syscall ABI:
//! - x8 = syscall number
//! - x0-x5 = arguments
//! - x0 = return value

use core::arch::asm;

/// Initialize SVC handling
pub fn init() {
    log::info!("[SVC] ARM64 syscall handler initialized");
    // No specific MSR setup needed like x86's SYSCALL
    // ARM64 uses exception vectors (VBAR_EL1)
}

/// Handle SVC exception from userspace
/// Called from exception.rs when ESR_EL1.EC == 0x15
pub fn handle_svc() {
    // Read syscall arguments from saved registers
    // In a real implementation, we'd save/restore the full context
    
    let (nr, arg0, arg1, arg2): (usize, usize, usize, usize);
    
    unsafe {
        // These would normally come from saved context
        // For now, read directly (simplified)
        asm!(
            "mov {nr}, x8",
            "mov {a0}, x0",
            "mov {a1}, x1",
            "mov {a2}, x2",
            nr = out(reg) nr,
            a0 = out(reg) arg0,
            a1 = out(reg) arg1,
            a2 = out(reg) arg2,
        );
    }
    
    // Dispatch to Rust syscall handler
    let result = crate::syscall::dispatch(nr, arg0, arg1, arg2);
    
    // Return value in x0
    unsafe {
        asm!(
            "mov x0, {result}",
            result = in(reg) result as u64,
        );
    }
    
    // Return to userspace via eret (done by exception return)
}

/// ARM64 syscall dispatcher (alternative entry point)
#[no_mangle]
pub extern "C" fn syscall_dispatch_arm64(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    _arg3: usize,
    _arg4: usize,
) -> isize {
    crate::syscall::dispatch(nr, arg0, arg1, arg2)
}
