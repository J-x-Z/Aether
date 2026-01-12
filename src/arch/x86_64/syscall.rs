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
/// Syscall args (Linux): RAX=nr, RDI=a0, RSI=a1, RDX=a2, R10=a3, R8=a4, R9=a5
#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // =================================================================================
        // FULL CONTEXT SAVE
        // Linux Syscall ABI: Kernel must preserve ALL regs except RAX, RCX, R11.
        // We must switch to kernel stack and save everything.
        // =================================================================================
        
        // NO DEBUG HERE - serial port polling hangs on hardware without COM1!
        // Debug output will be done in syscall_dispatch (via console_print_char)
        
        // 1. Save R15 to User Stack (so we can use R15 as scratch)
        // We push R15 to user stack. Then we save RSP (which points to saved R15).
        "push r15", 
        
        // 2. Save User RSP (now pointing to saved R15) to R15 register
        "mov r15, rsp",
        
        // 3. Switch to Kernel Stack
        // Use consistent LEA + MOV approach to safely load from static
        "mov rsp, [rip + KERNEL_RSP0]",
        
        // 4. Save User RSP (the value in R15)
        "push r15",
        
        // 5. Save everything else
        "push r14",
        "push r13",
        "push r12",
        "push rbp",
        "push rbx",
        "push r11", // User RFLAGS
        "push r10",
        "push r9",
        "push r8", 
        "push rcx", // User RIP
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax", // Syscall NR
        
        // Stack Frame is setup.
        
        // 6. Prepare Arguments for syscall_dispatch(nr, a0, a1, a2)
        // Windows ABI: RCX, RDX, R8, R9
        // Sources (from stack):
        //   NR (RAX) -> RCX
        //   a0 (RDI) -> RDX
        //   a1 (RSI) -> R8
        //   a2 (RDX) -> R9
        // Stack layout (top to bottom):
        // [RAX, RDI, RSI, RDX, RCX, R8, R9, R10, R11, RBX, RBP, R12, R13, R14, UserRSP]
        // Offsets from RSP:
        // RSP+0  = RAX
        // RSP+8  = RDI
        // RSP+16 = RSI
        // RSP+24 = RDX
        
        "mov rcx, [rsp]",      // NR (RAX)
        "mov rdx, [rsp+8]",    // a0 (RDI)
        "mov r8, [rsp+16]",    // a1 (RSI)
        "mov r9, [rsp+24]",    // a2 (RDX)
        
        // Shadow Space for MS x64 ABI (32 bytes)
        "sub rsp, 32",
        
        // 7. Call Dispatcher
        "call syscall_dispatch",
        
        // Restore Stack
        "add rsp, 32",
        
        // 8. Handle Return Value (RAX)
        // Overwrite saved RAX on stack with return value
        "mov [rsp], rax",
        
        // 9. Restore Registers
        "pop rax", // Restore RAX (now holding return value)
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx", // User RIP
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11", // User RFLAGS
        "pop rbx",
        "pop rbp",
        "pop r12",
        "pop r13",
        "pop r14",
        
        // 10. Restore User RSP
        "pop r15",      // This is (UserVal - 8)
        "mov rsp, r15", // Switch back to User Stack
        
        // 11. Restore R15 (it was pushed to user stack at step 1)
        "pop r15",      // RSP goes back to OriginalUserVal
        
        // 12. Return
        "sysretq",
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
    // Normal Dispatch
    // log::debug!("[Syscall] nr={} a0=0x{:x} a1=0x{:x} a2=0x{:x}", nr, arg0, arg1, arg2);
    crate::syscall::dispatch(nr, arg0, arg1, arg2)
}
