#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

/// Syscall numbers
const SYS_WRITE: usize = 1;
const SYS_EXIT: usize = 60;

// ================================================================================
// x86_64 Syscall Wrappers
// ================================================================================

#[cfg(target_arch = "x86_64")]
unsafe fn syscall1(nr: usize, arg0: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") nr,
        in("rdi") arg0,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

#[cfg(target_arch = "x86_64")]
unsafe fn syscall3(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") nr,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        out("rcx") _,
        out("r11") _,
        lateout("rax") ret,
    );
    ret
}

// ================================================================================
// ARM64 (AArch64) Syscall Wrappers
// ================================================================================

#[cfg(target_arch = "aarch64")]
unsafe fn syscall1(nr: usize, arg0: usize) -> isize {
    let ret: isize;
    asm!(
        "mov x8, {nr}",
        "mov x0, {arg0}",
        "svc #0",
        "mov {ret}, x0",
        nr = in(reg) nr,
        arg0 = in(reg) arg0,
        ret = out(reg) ret,
        out("x8") _,
        out("x0") _,
    );
    ret
}

#[cfg(target_arch = "aarch64")]
unsafe fn syscall3(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    asm!(
        "mov x8, {nr}",
        "mov x0, {arg0}",
        "mov x1, {arg1}",
        "mov x2, {arg2}",
        "svc #0",
        "mov {ret}, x0",
        nr = in(reg) nr,
        arg0 = in(reg) arg0,
        arg1 = in(reg) arg1,
        arg2 = in(reg) arg2,
        ret = out(reg) ret,
        out("x8") _,
        out("x0") _,
        out("x1") _,
        out("x2") _,
    );
    ret
}

// ================================================================================
// Entry Point
// ================================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = "Hello from Userspace (Ring 3 / EL0)!\n";
    unsafe {
        // write(1, msg, len)
        syscall3(SYS_WRITE, 1, msg.as_ptr() as usize, msg.len());
        
        // exit(0)
        syscall1(SYS_EXIT, 0);
    }
    
    // Should not reach here
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe { asm!("hlt"); }
        #[cfg(target_arch = "aarch64")]
        unsafe { asm!("wfi"); }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe { asm!("hlt"); }
        #[cfg(target_arch = "aarch64")]
        unsafe { asm!("wfi"); }
    }
}
