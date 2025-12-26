#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

/// Syscall numbers
const SYS_WRITE: usize = 1;
const SYS_EXIT: usize = 60;

/// Minimal Syscall Wrapper
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

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = "Hello from Userspace (Ring 3)!\n";
    unsafe {
        // write(1, msg, len)
        syscall3(SYS_WRITE, 1, msg.as_ptr() as usize, msg.len());
        
        // exit(0)
        syscall1(SYS_EXIT, 0);
    }
    
    // Should not reach here
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
