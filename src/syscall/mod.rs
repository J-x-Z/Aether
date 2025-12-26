//! POSIX Syscall Interface

mod elf;

use crate::sched::queue::CURRENT_TASK;
use crate::sched::task::FileDescriptor;
use crate::fs;
use alloc::string::String;
use alloc::vec::Vec;

/// Syscall numbers (Linux x86_64 ABI compatible)
pub mod numbers {
    // Core I/O
    pub const SYS_READ: usize = 0;
    pub const SYS_WRITE: usize = 1;
    pub const SYS_OPEN: usize = 2;
    pub const SYS_CLOSE: usize = 3;
    pub const SYS_STAT: usize = 4;
    pub const SYS_FSTAT: usize = 5;
    pub const SYS_LSEEK: usize = 8;
    pub const SYS_MMAP: usize = 9;
    pub const SYS_BRK: usize = 12;
    pub const SYS_IOCTL: usize = 16;
    
    // File descriptors
    pub const SYS_DUP: usize = 32;
    pub const SYS_DUP2: usize = 33;
    pub const SYS_PIPE: usize = 22;
    
    // Process
    pub const SYS_GETPID: usize = 39;
    pub const SYS_CLONE: usize = 56;
    pub const SYS_FORK: usize = 57;
    pub const SYS_EXECVE: usize = 59;
    pub const SYS_EXIT: usize = 60;
    pub const SYS_WAIT4: usize = 61;
    
    // Time
    pub const SYS_GETTIMEOFDAY: usize = 96;
    pub const SYS_NANOSLEEP: usize = 35;
    pub const SYS_CLOCK_GETTIME: usize = 228;
    
    // Memory
    pub const SYS_MUNMAP: usize = 11;
    
    // Misc
    pub const SYS_UNAME: usize = 63;
    pub const SYS_GETCWD: usize = 79;
    pub const SYS_CHDIR: usize = 80;
    pub const SYS_GETUID: usize = 102;
    pub const SYS_GETGID: usize = 104;
    pub const SYS_GETEUID: usize = 107;
    pub const SYS_GETEGID: usize = 108;
}

/// Main syscall dispatcher
pub fn dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    match nr {
        // Core I/O
        numbers::SYS_READ => sys_read(arg0, arg1, arg2),
        numbers::SYS_WRITE => sys_write(arg0, arg1, arg2),
        numbers::SYS_OPEN => sys_open(arg0, arg1, arg2),
        numbers::SYS_CLOSE => sys_close(arg0),
        numbers::SYS_STAT => sys_stat(arg0, arg1),
        numbers::SYS_FSTAT => sys_fstat(arg0, arg1),
        numbers::SYS_LSEEK => sys_lseek(arg0, arg1 as i64, arg2),
        numbers::SYS_MMAP => sys_mmap(arg0, arg1, arg2),
        numbers::SYS_MUNMAP => sys_munmap(arg0, arg1),
        numbers::SYS_BRK => sys_brk(arg0),
        numbers::SYS_IOCTL => sys_ioctl(arg0, arg1, arg2),
        
        // File descriptors
        numbers::SYS_DUP => sys_dup(arg0),
        numbers::SYS_DUP2 => sys_dup2(arg0, arg1),
        numbers::SYS_PIPE => sys_pipe(arg0),
        
        // Process
        numbers::SYS_GETPID => sys_getpid(),
        numbers::SYS_FORK => sys_fork(),
        numbers::SYS_CLONE => sys_clone(arg0, arg1, arg2),
        numbers::SYS_EXECVE => sys_execve(arg0, arg1, arg2),
        numbers::SYS_EXIT => sys_exit(arg0),
        numbers::SYS_WAIT4 => sys_wait4(arg0 as i32, arg1, arg2),
        
        // Time
        numbers::SYS_GETTIMEOFDAY => sys_gettimeofday(arg0, arg1),
        numbers::SYS_NANOSLEEP => sys_nanosleep(arg0, arg1),
        numbers::SYS_CLOCK_GETTIME => sys_clock_gettime(arg0, arg1),
        
        // Misc
        numbers::SYS_UNAME => sys_uname(arg0),
        numbers::SYS_GETCWD => sys_getcwd(arg0, arg1),
        numbers::SYS_CHDIR => sys_chdir(arg0),
        numbers::SYS_GETUID => sys_getuid(),
        numbers::SYS_GETGID => sys_getgid(),
        numbers::SYS_GETEUID => sys_geteuid(),
        numbers::SYS_GETEGID => sys_getegid(),
        
        _ => {
            log::warn!("[syscall] Unimplemented syscall: {}", nr);
            -38 // ENOSYS
        }
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

// ============================================================================
// File Syscalls (Phase 14 - POSIX)
// ============================================================================

fn sys_close(fd: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        if fd < task.fd_table.len() {
            task.fd_table[fd] = None;
            return 0;
        }
    }
    -9 // EBADF
}

fn sys_stat(_path: usize, _statbuf: usize) -> isize {
    // TODO: Implement stat - for now return stub
    log::debug!("[syscall::stat] Stub - returning success");
    0
}

fn sys_fstat(fd: usize, statbuf: usize) -> isize {
    // Write a minimal stat structure
    if statbuf != 0 {
        unsafe {
            let buf = statbuf as *mut u64;
            // Minimal stat: just set st_mode to regular file (0100644)
            *buf.add(1) = 0o100644; // st_mode at offset 8
            // Set st_size to 0
            *buf.add(6) = 0; // st_size at offset 48
        }
    }
    log::debug!("[syscall::fstat] fd={} - returning stub", fd);
    0
}

fn sys_lseek(fd: usize, offset: i64, whence: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        if let Some(file_opt) = task.fd_table.get_mut(fd) {
            if let Some(file) = file_opt {
                match whence {
                    0 => file.offset = offset as u64,           // SEEK_SET
                    1 => file.offset = (file.offset as i64 + offset) as u64, // SEEK_CUR
                    2 => { /* SEEK_END - would need file size */ }
                    _ => return -22, // EINVAL
                }
                return file.offset as isize;
            }
        }
    }
    -9 // EBADF
}

fn sys_ioctl(_fd: usize, cmd: usize, _arg: usize) -> isize {
    // Common ioctl commands - return success for terminal queries
    match cmd {
        0x5401 => 0,  // TCGETS - pretend we're a terminal
        0x5402 => 0,  // TCSETS
        0x5413 => {   // TIOCGWINSZ - get window size
            // Would fill in winsize struct if arg is valid
            0
        }
        _ => {
            log::debug!("[syscall::ioctl] Unknown cmd: 0x{:x}", cmd);
            -25 // ENOTTY
        }
    }
}

fn sys_dup(oldfd: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        if let Some(file_opt) = task.fd_table.get(oldfd) {
            if let Some(file) = file_opt.clone() {
                return task.add_file(file) as isize;
            }
        }
    }
    -9 // EBADF
}

fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        if let Some(file_opt) = task.fd_table.get(oldfd) {
            if let Some(file) = file_opt.clone() {
                // Extend table if needed
                while task.fd_table.len() <= newfd {
                    task.fd_table.push(None);
                }
                task.fd_table[newfd] = Some(file);
                return newfd as isize;
            }
        }
    }
    -9 // EBADF
}

fn sys_pipe(_pipefd: usize) -> isize {
    log::warn!("[syscall::pipe] Pipe not implemented");
    -38 // ENOSYS
}

fn sys_munmap(_addr: usize, _length: usize) -> isize {
    // Stub - pretend to unmap
    log::debug!("[syscall::munmap] Stub - returning success");
    0
}

// ============================================================================
// Process Syscalls
// ============================================================================

/// Fork - Create child process
/// Returns 0 in child, child PID in parent
fn sys_fork() -> isize {
    log::info!("[syscall::fork] Creating child process...");
    
    // Get current task
    let current_lock = CURRENT_TASK.lock();
    let current_arc = match current_lock.as_ref() {
        Some(t) => t.clone(),
        None => {
            log::warn!("[syscall::fork] No current task");
            return -1;
        }
    };
    drop(current_lock);
    
    let parent = current_arc.lock();
    let parent_pid = parent.id;
    
    // For now, create a simple fork by copying the parent's state
    // In a real implementation, we'd need to:
    // 1. Copy page tables (or set up CoW)
    // 2. Save current CPU context
    // 3. Create child with modified context (return 0)
    
    // Get return address from stack (simplified - assumes called from syscall)
    // In a real implementation, this comes from the saved context
    let child_rip = 0u64; // Will be set by context switch
    let child_rsp = 0u64;
    
    // Create child task
    let child = parent.fork(child_rsp, child_rip);
    let child_pid = child.id;
    
    drop(parent);
    
    // Add child to scheduler
    crate::sched::queue::spawn_task(child);
    
    log::info!("[syscall::fork] Created child PID {} from parent PID {}", child_pid, parent_pid);
    
    // Parent returns child PID
    // Note: Without a real scheduler, child never runs!
    // This is a simplified implementation for testing
    child_pid as isize
}

fn sys_clone(_flags: usize, _stack: usize, _parent_tid: usize) -> isize {
    // clone is similar to fork but with more options
    // For now, just call fork
    log::info!("[syscall::clone] Using fork implementation");
    sys_fork()
}

fn sys_execve(pathname: usize, argv: usize, _envp: usize) -> isize {
    // Get pathname string
    let path = unsafe { get_user_string(pathname, 0) };
    if path.is_none() {
        log::warn!("[syscall::execve] Invalid pathname");
        return -14; // EFAULT
    }
    let path = path.unwrap();
    
    log::info!("[syscall::execve] Loading: {}", path);
    
    // Open the file
    let inode = match fs::open(&path, 0) {
        Ok(inode) => inode,
        Err(_) => {
            log::warn!("[syscall::execve] File not found: {}", path);
            return -2; // ENOENT
        }
    };
    
    // Read file contents
    let mut buffer = alloc::vec![0u8; 65536]; // 64KB max for now
    let len = inode.read_at(0, &mut buffer);
    
    if len == 0 {
        log::warn!("[syscall::execve] Empty file");
        return -8; // ENOEXEC
    }
    
    // Load ELF
    let loaded = match elf::load_elf(&buffer[..len]) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("[syscall::execve] ELF load error: {}", e);
            return -8; // ENOEXEC
        }
    };
    
    log::info!("[syscall::execve] ELF loaded, entry: 0x{:x}", loaded.entry_point);
    
    // Parse argv
    let mut argv_vec: Vec<&[u8]> = Vec::new();
    if argv != 0 {
        unsafe {
            let mut ptr = argv as *const usize;
            while *ptr != 0 {
                let arg_ptr = *ptr as *const u8;
                let mut len = 0;
                while *arg_ptr.add(len) != 0 {
                    len += 1;
                    if len > 1024 { break; }
                }
                argv_vec.push(core::slice::from_raw_parts(arg_ptr, len));
                ptr = ptr.add(1);
            }
        }
    }
    
    // For simplicity, use empty envp for now
    let envp_vec: Vec<&[u8]> = Vec::new();
    
    // Set up new stack at 0x7FFFFF000000 (typical Linux user stack area)
    let stack_top = 0x7FFFFF000000u64;
    let stack_size = 8 * 4096; // 32KB stack
    crate::mm::paging::make_user_accessible(stack_top - stack_size, stack_size);
    
    // Set up stack with argv/envp
    let user_sp = elf::setup_user_stack(stack_top, &argv_vec, &envp_vec);
    
    log::info!("[syscall::execve] Stack at 0x{:x}, entering usermode...", user_sp);
    
    // Jump to new program
    // Note: This replaces the current "process" - we never return
    #[cfg(target_arch = "x86_64")]
    unsafe {
        crate::arch::x86_64::enter_usermode(loaded.entry_point, user_sp);
    }
    
    #[cfg(target_arch = "aarch64")]
    unsafe {
        crate::arch::aarch64::enter_usermode(loaded.entry_point, user_sp);
    }
    
    // Should never reach here
    -1
}

fn sys_wait4(_pid: i32, _wstatus: usize, _options: usize) -> isize {
    log::warn!("[syscall::wait4] Wait4 not implemented");
    -10 // ECHILD - no child processes
}

// ============================================================================
// Time Syscalls
// ============================================================================

static mut BOOT_TIME: u64 = 0;

fn sys_gettimeofday(tv: usize, _tz: usize) -> isize {
    if tv != 0 {
        unsafe {
            let timeval = tv as *mut u64;
            // Fake time: return boot time + some counter
            BOOT_TIME += 1;
            *timeval = BOOT_TIME;        // tv_sec
            *timeval.add(1) = 0;         // tv_usec
        }
    }
    0
}

fn sys_nanosleep(req: usize, _rem: usize) -> isize {
    if req != 0 {
        // Read timespec but just spin for now
        // In real OS we'd schedule another task
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
    0
}

fn sys_clock_gettime(clock_id: usize, tp: usize) -> isize {
    if tp != 0 {
        unsafe {
            let timespec = tp as *mut u64;
            BOOT_TIME += 1;
            *timespec = BOOT_TIME;        // tv_sec
            *timespec.add(1) = 0;         // tv_nsec
        }
    }
    log::debug!("[syscall::clock_gettime] clock_id={}", clock_id);
    0
}

// ============================================================================
// Misc Syscalls
// ============================================================================

fn sys_uname(buf: usize) -> isize {
    if buf != 0 {
        unsafe {
            let ptr = buf as *mut u8;
            // struct utsname: 5 fields of 65 bytes each
            let sysname = b"Aether\0";
            let nodename = b"aether\0";
            let release = b"0.1.0\0";
            let version = b"#1 SMP\0";
            let machine = b"x86_64\0";
            
            core::ptr::copy_nonoverlapping(sysname.as_ptr(), ptr, sysname.len());
            core::ptr::copy_nonoverlapping(nodename.as_ptr(), ptr.add(65), nodename.len());
            core::ptr::copy_nonoverlapping(release.as_ptr(), ptr.add(130), release.len());
            core::ptr::copy_nonoverlapping(version.as_ptr(), ptr.add(195), version.len());
            core::ptr::copy_nonoverlapping(machine.as_ptr(), ptr.add(260), machine.len());
        }
    }
    0
}

fn sys_getcwd(buf: usize, size: usize) -> isize {
    if buf != 0 && size > 1 {
        unsafe {
            let ptr = buf as *mut u8;
            *ptr = b'/';
            *ptr.add(1) = 0;
        }
        return buf as isize;
    }
    -34 // ERANGE
}

fn sys_chdir(_path: usize) -> isize {
    // Stub - pretend to change directory
    log::debug!("[syscall::chdir] Stub - returning success");
    0
}

fn sys_getuid() -> isize { 0 }   // root
fn sys_getgid() -> isize { 0 }   // root
fn sys_geteuid() -> isize { 0 }  // root
fn sys_getegid() -> isize { 0 }  // root
