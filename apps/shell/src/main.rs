#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

// ============================================================================
// Syscall Numbers (Linux x86_64 ABI)
// ============================================================================

const SYS_READ: usize = 0;
const SYS_WRITE: usize = 1;
const SYS_EXIT: usize = 60;
const SYS_GETPID: usize = 39;

// ============================================================================
// Syscall Wrappers
// ============================================================================

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

fn write(fd: usize, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

fn read(fd: usize, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd, buf.as_ptr() as usize, buf.len()) }
}

fn exit(code: usize) -> ! {
    unsafe { syscall1(SYS_EXIT, code) };
    loop {}
}

fn getpid() -> isize {
    unsafe { syscall1(SYS_GETPID, 0) }
}

fn print(s: &str) {
    write(1, s.as_bytes());
}

fn println(s: &str) {
    print(s);
    print("\n");
}

// ============================================================================
// Simple Shell
// ============================================================================

const PROMPT: &str = "aether> ";
const MAX_INPUT: usize = 256;

fn streq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    
    while start < end && (s[start] == b' ' || s[start] == b'\n' || s[start] == b'\r') {
        start += 1;
    }
    while end > start && (s[end-1] == b' ' || s[end-1] == b'\n' || s[end-1] == b'\r') {
        end -= 1;
    }
    
    &s[start..end]
}

fn process_command(input: &[u8]) {
    let cmd = trim(input);
    
    if cmd.is_empty() {
        return;
    }
    
    // Built-in commands
    if streq(cmd, b"exit") {
        println("Goodbye!");
        exit(0);
    } else if streq(cmd, b"help") {
        println("Built-in commands:");
        println("  help  - Show this help");
        println("  echo  - Echo arguments");
        println("  pid   - Show process ID");
        println("  exit  - Exit shell");
    } else if cmd.starts_with(b"echo ") {
        // Echo the rest of the line
        let rest = &cmd[5..];
        write(1, rest);
        print("\n");
    } else if streq(cmd, b"echo") {
        print("\n");
    } else if streq(cmd, b"pid") {
        let pid = getpid();
        print("PID: ");
        // Simple number printing (single digit for now)
        let digit = (pid as u8) + b'0';
        write(1, &[digit]);
        print("\n");
    } else {
        print("Unknown command: ");
        write(1, cmd);
        print("\n");
        println("Type 'help' for available commands.");
    }
}

// ============================================================================
// Entry Point
// ============================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println("Aether Shell v0.1");
    println("Type 'help' for available commands.");
    println("");
    
    let mut input_buf = [0u8; MAX_INPUT];
    let mut input_len = 0usize;
    
    loop {
        print(PROMPT);
        
        // Read line (simplified - assumes read returns full line)
        input_len = 0;
        loop {
            let mut ch = [0u8; 1];
            let n = read(0, &mut ch);
            if n <= 0 {
                // No input available, yield
                continue;
            }
            
            if ch[0] == b'\n' || ch[0] == b'\r' {
                input_buf[input_len] = 0;
                print("\n");
                break;
            }
            
            if input_len < MAX_INPUT - 1 {
                input_buf[input_len] = ch[0];
                input_len += 1;
                // Echo character
                write(1, &ch);
            }
        }
        
        process_command(&input_buf[..input_len]);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println("Shell panic!");
    exit(1);
}
