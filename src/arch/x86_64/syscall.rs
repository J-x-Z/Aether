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
        // CRITICAL: Must preserve RCX (user RIP) and R11 (user RFLAGS)
        // for sysretq to return correctly to userspace
        // ============================================================
        
        // Step 1: Save user RSP to r15, switch to kernel stack
        "mov r15, rsp",
        "lea rsp, [{kernel_rsp}]",
        "mov rsp, [rsp]",
        
        // Step 2: Save user state that sysretq needs
        "push r15",                         // User RSP
        "push rcx",                         // User RIP (CRITICAL - for sysretq)
        "push r11",                         // User RFLAGS (CRITICAL - for sysretq)
        
        // Step 3: Save syscall args to callee-saved registers
        // (so they survive the function call)
        "mov r15, rax",                     // r15 = syscall number
        "mov r14, rdi",                     // r14 = a0
        "mov r13, rsi",                     // r13 = a1
        "mov r12, rdx",                     // r12 = a2 (save before it becomes arg2)
        // r10, r8, r9 will become arg3-5, no need to save
        
        // Step 4: Set up C calling convention
        // syscall_dispatch(nr, a0, a1, a2, a3, a4, a5)
        // C: RDI, RSI, RDX, RCX, R8, R9
        "mov rdi, r15",                     // RDI = syscall number
        "mov rsi, r14",                     // RSI = a0
        "mov rdx, r13",                     // RDX = a1
        "mov rcx, r12",                     // RCX = a2
        "mov r8, r10",                      // R8 = a3 (syscall a3 is in r10)
        // R9 = a4 stays in r8? No, syscall a4 is already in r8
        // Actually: syscall uses r8=a4, r9=a5
        // So we need: C_r8 = syscall_a3 (r10), C_r9 = syscall_a4 (r8)
        // But r8 already has syscall_a4, and we need r8 for C_a3
        // This is tricky... let's save r8 first
        "xchg r8, r10",                     // Now r8=a3, r10=a4
        "mov r9, r10",                      // R9 = a4
        
        // Step 5: Call the Rust dispatcher
        "call syscall_dispatch",
        
        // RAX = return value
        
        // Step 6: Restore user state for sysretq
        // sysretq uses: RCX=user RIP, R11=user RFLAGS
        "pop r11",                          // User RFLAGS
        "pop rcx",                          // User RIP
        "pop rsp",                          // User RSP
        
        // Step 7: Return to userspace
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
