//! POSIX Syscall Interface

/// Syscall numbers (Linux x86_64 ABI compatible)
pub mod numbers {
    pub const SYS_READ: usize = 0;
    pub const SYS_WRITE: usize = 1;
    pub const SYS_OPEN: usize = 2;
    pub const SYS_CLOSE: usize = 3;
    pub const SYS_EXIT: usize = 60;
    pub const SYS_FORK: usize = 57;
    pub const SYS_EXECVE: usize = 59;
}

/// Main syscall dispatcher
/// Called from syscall instruction handler
pub fn dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    match nr {
        numbers::SYS_READ => sys_read(arg0, arg1, arg2),
        numbers::SYS_WRITE => sys_write(arg0, arg1, arg2),
        numbers::SYS_EXIT => sys_exit(arg0),
        _ => -1, // ENOSYS
    }
}

fn sys_read(_fd: usize, _buf: usize, _count: usize) -> isize {
    // TODO: Implement
    -1
}

fn sys_write(fd: usize, buf: usize, count: usize) -> isize {
    if fd == 1 || fd == 2 {
        // stdout/stderr - print to console
        unsafe {
            let slice = core::slice::from_raw_parts(buf as *const u8, count);
            if let Ok(s) = core::str::from_utf8(slice) {
                log::info!("[syscall::write] {}", s);
            }
        }
        return count as isize;
    }
    -1
}

fn sys_exit(code: usize) -> isize {
    log::info!("[syscall::exit] Process exited with code {}", code);
    loop {} // TODO: Proper process termination
}
