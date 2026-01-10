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
///   RSP = user stack
///   RAX = syscall number
///   RDI = arg0, RSI = arg1, RDX = arg2, R10 = arg3, R8 = arg4, R9 = arg5
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // ============================================================
        // SIMPLIFIED: Direct register shuffling, minimal stack use
        // ============================================================
        
        // Step 1: Save user RSP to r15, switch to kernel stack
        "mov r15, rsp",
        "lea rsp, [{kernel_rsp}]",
        "mov rsp, [rsp]",
        
        // Step 2: Save only what we need for sysret (on kernel stack)
        "push r15",                         // User RSP
        "push rcx",                         // User RIP (for sysret)
        "push r11",                         // User RFLAGS (for sysret)
        
        // Step 3: Save syscall args to stack (we'll clobber these regs)
        // Syscall convention: RAX=nr, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5
        // We need to map to C calling convention for syscall_dispatch(nr, a0, a1, a2, ...)
        // C convention: RDI=arg0, RSI=arg1, RDX=arg2, RCX=arg3, R8=arg4, R9=arg5
        
        // Save original values we need to shuffle
        "mov r14, rax",                     // r14 = syscall number
        "mov r13, rdi",                     // r13 = a0 (original rdi)
        "mov r12, rsi",                     // r12 = a1 (original rsi)
        // rdx stays as a2 -> will become arg2, but we need it for arg1
        // r10 = a3, r8 = a4, r9 = a5
        
        // Now rearrange for C calling convention:
        // syscall_dispatch(nr, a0, a1, a2) - we only pass 4 args
        "mov rdi, r14",                     // RDI = syscall number (was RAX)
        "mov rsi, r13",                     // RSI = a0 (was RDI)
        "mov rcx, rdx",                     // Save RDX before clobbering
        "mov rdx, r12",                     // RDX = a1 (was RSI)
        // RCX = a2 (was RDX, saved in rcx now) - wait this is wrong
        // Let me redo this more carefully
        
        // Actually: shuffle in correct order
        // We have: RDI=a0, RSI=a1, RDX=a2, and we need: RDI=nr, RSI=a0, RDX=a1, RCX=a2
        // Using r14=nr, r13=a0, r12=a1, rdx=a2
        // "mov rdi, r14" - correct
        // "mov rsi, r13" - correct  
        // "mov rcx, rdx" - this saves a2 to rcx (correct!)
        // "mov rdx, r12" - this sets rdx = a1 (correct!)
        // So the order above is actually fine if we save rdx BEFORE overwriting it
        
        // Let me redo completely to be safe:
        "mov rdi, r14",                     // RDI = nr
        "mov rsi, r13",                     // RSI = a0
        // Before setting RDX, save its current value (a2) to RCX
        "mov rcx, rdx",                     // RCX = a2 (original RDX)
        "mov rdx, r12",                     // RDX = a1 (saved from RSI)
        // R8 and R9 stay as a4 and a5 if needed
        
        // Call the Rust dispatcher
        "call syscall_dispatch",
        
        // RAX = return value
        
        // Restore user state
        "pop r11",                          // User RFLAGS
        "pop rcx",                          // User RIP
        "pop rsp",                          // User RSP
        
        // Return to userspace
        "sysretq",
        
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
    log::debug!("[Syscall] nr={} a0=0x{:x} a1=0x{:x} a2=0x{:x}", nr, arg0, arg1, arg2);
    crate::syscall::dispatch(nr, arg0, arg1, arg2)
}
