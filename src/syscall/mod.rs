//! POSIX Syscall Interface

use crate::sched::queue::CURRENT_TASK;
use crate::sched::task::FileDescriptor;
use crate::fs;
use alloc::string::String;

/// Syscall numbers (Linux x86_64 ABI compatible)
pub mod numbers {
    pub const SYS_READ: usize = 0;
    pub const SYS_WRITE: usize = 1;
    pub const SYS_OPEN: usize = 2;
    pub const SYS_CLOSE: usize = 3;
    pub const SYS_EXIT: usize = 60;
}

/// Main syscall dispatcher
pub fn dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    match nr {
        numbers::SYS_READ => sys_read(arg0, arg1, arg2),
        numbers::SYS_WRITE => sys_write(arg0, arg1, arg2),
        numbers::SYS_OPEN => sys_open(arg0, arg1, arg2),
        numbers::SYS_EXIT => sys_exit(arg0),
        _ => -1, // ENOSYS
    }
}

// Helper to get string from user pointer
unsafe fn get_user_string(ptr: usize, _len: usize) -> Option<String> {
    // TODO: Verify user pointer access rights
    // For now, assume null-terminated if len not provided, or fixed length
    // But SYS_OPEN passes filename ptr, not len.
    // We need to scan for null or limit.
    let ptr = ptr as *const u8;
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
        if len > 1024 { return None; } // Safety limit
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    String::from_utf8(slice.to_vec()).ok()
}

fn sys_open(filename: usize, flags: usize, _mode: usize) -> isize {
    let filename = unsafe { get_user_string(filename, 0) };
    if filename.is_none() { return -2; } // ENOENT/EFAULT
    let filename = filename.unwrap();

    // Call VFS open
    match fs::open(&filename, flags as u32) {
        Ok(inode) => {
            let fd = FileDescriptor {
                inode,
                offset: 0,
                flags: flags as u32,
            };
            
            // Add to current task
            let current_lock = CURRENT_TASK.lock();
            if let Some(task_arc) = current_lock.as_ref() {
                let mut task = task_arc.lock();
                task.add_file(fd) as isize
            } else {
                -1 // EACCES (No task)
            }
        },
        Err(_) => -2, // ENOENT
    }
}

fn sys_read(fd: usize, buf_ptr: usize, count: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        if let Some(file_opt) = task.fd_table.get_mut(fd) {
            if let Some(file) = file_opt {
                let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, count) };
                let bytes = file.inode.read_at(file.offset, buf);
                file.offset += bytes as u64;
                return bytes as isize;
            }
        }
    }
    -9 // EBADF
}

fn sys_write(fd: usize, buf_ptr: usize, count: usize) -> isize {
    // Special handling for stdout/stderr (created empty in task)
    if fd == 1 || fd == 2 {
        unsafe {
            let slice = core::slice::from_raw_parts(buf_ptr as *const u8, count);
            if let Ok(s) = core::str::from_utf8(slice) {
                // Use kernel console for now
                // Since this is bare metal, we use console_println from aether-user or just log
                log::info!("[STDOUT] {}", s);
            }
        }
        return count as isize;
    }

    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
         if let Some(file_opt) = task.fd_table.get_mut(fd) {
            if let Some(file) = file_opt {
                let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, count) };
                let bytes = file.inode.write_at(file.offset, buf);
                file.offset += bytes as u64;
                return bytes as isize;
            }
        }
    }
    -9 // EBADF
}

fn sys_exit(code: usize) -> isize {
    log::info!("[syscall::exit] Process exited with code {}", code);
    
    // Update task state
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        task.state = crate::sched::task::TaskState::Terminated;
    }
    
    // Trigger scheduler (TODO)
    loop {
        // Halt cpu to simplify
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("hlt") };
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("wfi") };
    }
}
