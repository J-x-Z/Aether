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
    pub const SYS_BRK: usize = 12;
    pub const SYS_GETPID: usize = 39;
    pub const SYS_FORK: usize = 57;
    pub const SYS_EXIT: usize = 60;
    pub const SYS_MMAP: usize = 9;
}

/// Main syscall dispatcher
pub fn dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    match nr {
        numbers::SYS_READ => sys_read(arg0, arg1, arg2),
        numbers::SYS_WRITE => sys_write(arg0, arg1, arg2),
        numbers::SYS_OPEN => sys_open(arg0, arg1, arg2),
        numbers::SYS_BRK => sys_brk(arg0),
        numbers::SYS_GETPID => sys_getpid(),
        numbers::SYS_FORK => sys_fork(),
        numbers::SYS_EXIT => sys_exit(arg0),
        numbers::SYS_MMAP => sys_mmap(arg0, arg1, arg2),
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

// ============================================================================
// Extended Syscalls (Phase 14)
// ============================================================================

/// Program break management (heap allocation)
/// For now, we use a simple linear allocator
static mut PROGRAM_BREAK: usize = 0x800000; // Start at 8MB

fn sys_brk(addr: usize) -> isize {
    unsafe {
        if addr == 0 {
            // Query current break
            return PROGRAM_BREAK as isize;
        }
        
        if addr >= 0x800000 && addr <= 0x1000000 {
            // Valid range (8MB - 16MB)
            let old_break = PROGRAM_BREAK;
            PROGRAM_BREAK = addr;
            
            // Make the new region user-accessible
            crate::mm::paging::make_user_accessible(old_break as u64, (addr - old_break) as u64);
            
            log::debug!("[syscall::brk] Program break: 0x{:x} -> 0x{:x}", old_break, addr);
            return addr as isize;
        }
        
        -12 // ENOMEM
    }
}

/// Get process ID
fn sys_getpid() -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let task = task_arc.lock();
        return task.id as isize;
    }
    1 // Default PID if no task
}

/// Fork - Create child process (stub for now)
fn sys_fork() -> isize {
    log::warn!("[syscall::fork] Fork not implemented, returning error");
    -38 // ENOSYS - Not implemented
}

/// Memory map (simplified stub)
fn sys_mmap(addr: usize, length: usize, _prot: usize) -> isize {
    // Simple anonymous mapping at requested address
    if addr == 0 {
        // Kernel chooses address
        unsafe {
            let new_addr = PROGRAM_BREAK;
            let aligned_len = (length + 4095) & !4095;
            PROGRAM_BREAK += aligned_len;
            
            crate::mm::paging::make_user_accessible(new_addr as u64, aligned_len as u64);
            log::debug!("[syscall::mmap] Mapped {} bytes at 0x{:x}", aligned_len, new_addr);
            return new_addr as isize;
        }
    }
    
    // Fixed address mapping
    let aligned_len = (length + 4095) & !4095;
    crate::mm::paging::make_user_accessible(addr as u64, aligned_len as u64);
    log::debug!("[syscall::mmap] Mapped {} bytes at 0x{:x} (fixed)", aligned_len, addr);
    addr as isize
}
