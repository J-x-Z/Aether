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

/// Initialize SYSCALL/SYSRET mechanism
pub fn init() {
    let k_cs = super::gdt::kernel_cs() as u64;
    let u_ds = super::gdt::user_ds() as u64;
    
    // For SYSRET: CS = STAR[63:48] + 16, SS = STAR[63:48] + 8.
    // Our GDT: UserData (idx 3), UserCode (idx 4).
    // So Base = UserData - 8.
    // Use (u_ds & !3) to get base index, then -8, then |3 (RPL).
    let sysret_base = ((u_ds & !3) - 8) | 3;
    
    unsafe {
        // STAR: [63:48] = User CS/SS base, [47:32] = Kernel CS/SS base
        let star = (sysret_base << 48) | (k_cs << 32);
        wrmsr(MSR_STAR, star);
        
        // LSTAR: Handler address
        wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);
        
        // SFMASK: Flags to clear on syscall (IF, TF, DF)
        wrmsr(MSR_SFMASK, 0x300); // Clear IF and DF
        
        // EFER: Enable System Call Extensions (SCE) - Bit 0 (Manual ASM)
        let msr = 0xC0000080u32;
        let mut low: u32;
        let mut high: u32;
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack));
        low |= 1; 
        asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nomem, nostack));
        
        // CR4.TSD (Time Stamp Disable) - Bit 2
        // Clear it to allow rdtsc in userspace (common cause of #GP(0))
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack));
        if (cr4 & 4) != 0 {
            cr4 &= !4;
            asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack));
        }
    }
    
    log::info!("[Syscall] x86_64 SYSCALL/SYSRET initialized (Asm EFER.SCE + CR4.TSD-OFF)");
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
/// 
/// At entry (from CPU):
///   RCX = user RIP (saved by CPU for sysret)
///   R11 = user RFLAGS (saved by CPU for sysret)
///   RSP = user stack (DANGER: we're in Ring 0 but on user stack!)
///   RAX = syscall number
///   RDI, RSI, RDX, R10, R8, R9 = syscall arguments
///
/// This function:
///   1. swapgs to get kernel GS base
///   2. Switch to kernel stack (from GS-based per-CPU storage)
///   3. Save user registers
///   4. Call Rust syscall dispatcher
///   5. Restore user registers
///   6. Switch back to user stack
///   7. swapgs back
///   8. sysretq
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // ============================================================
        // ENTRY: We're in Ring 0 but on USER STACK - be very careful!
        // ============================================================
        
        // Step 1: Swap GS (now GS points to kernel per-CPU data)
        "swapgs",
        
        // Step 2: Save user RSP to kernel per-CPU scratch, load kernel RSP
        // We use a global variable since we don't have proper per-CPU yet
        // KERNEL_SCRATCH_RSP is set before entering usermode
        "mov gs:[{scratch_rsp}], rsp",      // Save user RSP to scratch
        "mov rsp, gs:[{kernel_rsp}]",       // Load kernel RSP
        
        // Step 3: Now we're on kernel stack - safe to push
        // Save user registers that we need to preserve
        "push rcx",                         // User RIP
        "push r11",                         // User RFLAGS
        "push gs:[{scratch_rsp}]",          // User RSP (from scratch)
        
        // Save all callee-saved registers
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        
        // Step 4: Set up arguments for syscall_dispatch
        // RAX = syscall number (already there)
        // RDI = arg0 (already there)  
        // RSI = arg1 (already there)
        // RDX = arg2 (already there)
        // RCX = arg3 (need to move from R10)
        // R8 = arg4 (already there)
        // R9 = arg5 (already there)
        "mov rcx, r10",
        
        // Call the Rust dispatcher
        "call syscall_dispatch",
        
        // RAX now contains return value
        
        // Step 5: Restore callee-saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        
        // Step 6: Restore user RSP, RIP, RFLAGS
        "pop gs:[{scratch_rsp}]",           // User RSP to scratch
        "pop r11",                          // User RFLAGS
        "pop rcx",                          // User RIP
        
        // Switch back to user stack
        "mov rsp, gs:[{scratch_rsp}]",
        
        // Step 7: Swap GS back to user GS
        "swapgs",
        
        // Step 8: Return to userspace
        // sysretq loads RIP from RCX, RFLAGS from R11
        "sysretq",
        
        // Symbol references
        scratch_rsp = sym SCRATCH_USER_RSP,
        kernel_rsp = sym KERNEL_RSP0,
    );
}

/// Per-CPU data for syscall handling
/// These are accessed via GS segment in syscall_entry
/// For now, we use simple globals (single-CPU)
#[no_mangle]
pub static mut SCRATCH_USER_RSP: u64 = 0;

#[no_mangle]
pub static mut KERNEL_RSP0: u64 = 0;

/// Set up the kernel stack for syscall handling
/// Call this BEFORE entering usermode
pub fn setup_syscall_stacks(kernel_stack_top: u64) {
    unsafe {
        KERNEL_RSP0 = kernel_stack_top;
        
        // Set up GS base to point to our per-CPU data area
        // For simplicity, we set GS base to 0 and use absolute addresses
        // In a real OS, you'd use a per-CPU structure
        use x86_64::registers::model_specific::GsBase;
        GsBase::write(x86_64::VirtAddr::new(0));
    }
    log::debug!("[Syscall] Kernel RSP0 set to 0x{:x}", kernel_stack_top);
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
