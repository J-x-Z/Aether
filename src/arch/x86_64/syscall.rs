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
/// At entry: RCX=user RIP, R11=user RFLAGS, RSP=user stack
/// Syscall args: RAX=nr, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // ===== ULTRA-SIMPLE APPROACH =====
        // Save syscall args to callee-saved registers IMMEDIATELY
        // Syscall ABI (Linux): RAX=nr, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5
        
        // Step 1: Save syscall args
        "mov r12, rax",                     // r12 = syscall number
        "mov r13, rdi",                     // r13 = a0  
        "mov r14, rsi",                     // r14 = a1
        "mov r15, rdx",                     // r15 = a2
        
        // Step 2: Save user RSP to rbx (callee-saved)
        "mov rbx, rsp",
        
        // Step 3: Switch to kernel stack
        "lea rsp, [{kernel_rsp}]",
        "mov rsp, [rsp]",
        
        // Step 4: Push sysret state (on kernel stack)
        "push rbx",                         // User RSP
        "push rcx",                         // User RIP (syscall puts it here)
        "push r11",                         // User RFLAGS (syscall puts it here)
        
        // Step 5: Set up WINDOWS x64 calling convention for syscall_dispatch(nr, a0, a1, a2)
        // UEFI target uses Windows ABI: RCX, RDX, R8, R9 (NOT Linux: RDI, RSI, RDX, RCX)
        "mov rcx, r12",                     // RCX = syscall number
        "mov rdx, r13",                     // RDX = a0
        "mov r8, r14",                      // R8 = a1
        "mov r9, r15",                      // R9 = a2
        
        // Step 6: Call dispatcher
        "call syscall_dispatch",
        
        // RAX = return value
        
        // Step 7: Restore sysret state
        "pop r11",                          // User RFLAGS
        "pop rcx",                          // User RIP
        "pop rsp",                          // User RSP
        
        // Step 8: Return to userspace
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
#[inline(never)]
pub extern "C" fn syscall_dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    log::debug!("[Syscall] nr={} a0=0x{:x} a1=0x{:x} a2=0x{:x}", nr, arg0, arg1, arg2);
    crate::syscall::dispatch(nr, arg0, arg1, arg2)
}
