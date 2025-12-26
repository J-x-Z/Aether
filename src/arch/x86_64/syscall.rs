//! x86_64 Syscall Entry Point
//!
//! Linux x86_64 syscall ABI:
//! - rax = syscall number
//! - rdi = arg0, rsi = arg1, rdx = arg2, r10 = arg3, r8 = arg4, r9 = arg5
//! - Return value in rax
//!
//! This module sets up the SYSCALL/SYSRET mechanism via MSRs.

use core::arch::asm;

/// Model Specific Registers for SYSCALL
pub const MSR_STAR: u32 = 0xC0000081;     // Segment selectors
pub const MSR_LSTAR: u32 = 0xC0000082;    // RIP for syscall handler
pub const MSR_SFMASK: u32 = 0xC0000084;   // RFLAGS mask

/// Kernel code segment selector (from GDT)
const KERNEL_CS: u64 = 0x08;
/// Kernel data segment selector
const KERNEL_DS: u64 = 0x10;
/// User code segment selector  
const USER_CS: u64 = 0x1B;  // Ring 3, index 3
/// User data segment selector
const USER_DS: u64 = 0x23;  // Ring 3, index 4

/// Initialize SYSCALL/SYSRET mechanism
pub fn init() {
    unsafe {
        // STAR: [63:48] = User CS/SS base, [47:32] = Kernel CS/SS base
        // For SYSRET: CS = STAR[63:48] + 16, SS = STAR[63:48] + 8
        // For SYSCALL: CS = STAR[47:32], SS = STAR[47:32] + 8
        let star = ((USER_CS - 16) << 48) | (KERNEL_CS << 32);
        wrmsr(MSR_STAR, star);
        
        // LSTAR: Handler address
        wrmsr(MSR_LSTAR, syscall_entry as u64);
        
        // SFMASK: Flags to clear on syscall (IF, TF, DF)
        wrmsr(MSR_SFMASK, 0x300); // Clear IF and DF
    }
    
    log::info!("[Syscall] x86_64 SYSCALL/SYSRET initialized");
}

/// Write to Model Specific Register
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, nomem)
    );
}

/// Syscall entry point (naked function)
/// Called when userspace executes `syscall` instruction
#[naked]
#[no_mangle]
pub extern "C" fn syscall_entry() {
    unsafe {
        asm!(
            // Save user stack pointer (in rcx after syscall)
            // rcx = user RIP, r11 = user RFLAGS
            
            // Switch to kernel stack (TODO: Use per-CPU kernel stack)
            // For now, we use a simple approach
            
            // Push callee-saved registers
            "push rbx",
            "push rbp",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            
            // Save user RIP and RFLAGS
            "push rcx",  // User RIP
            "push r11",  // User RFLAGS
            
            // Arguments are already in correct registers for our dispatch
            // rax = syscall number
            // rdi = arg0, rsi = arg1, rdx = arg2, r10 = arg3, r8 = arg4, r9 = arg5
            
            // Move r10 to rcx for C calling convention (arg3)
            "mov rcx, r10",
            
            // Call Rust syscall dispatcher
            // fn syscall_dispatch(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize
            "call syscall_dispatch",
            
            // Return value is in rax
            
            // Restore user RFLAGS and RIP
            "pop r11",
            "pop rcx",
            
            // Restore callee-saved registers
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbp",
            "pop rbx",
            
            // Return to userspace
            "sysretq",
            
            options(noreturn)
        );
    }
}

/// Rust syscall dispatcher (called from assembly)
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    nr: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    _arg5: usize,
) -> isize {
    crate::syscall::dispatch(nr, arg0, arg1, arg2)
}
