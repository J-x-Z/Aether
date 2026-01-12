//! POSIX Syscall Interface

pub mod elf;
pub mod dynlink;

use crate::sched::queue::CURRENT_TASK;
use crate::sched::task::FileDescriptor;
use crate::fs;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 0x001;
pub const POLLPRI: i16 = 0x002;
pub const POLLOUT: i16 = 0x004;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IoVec {
    pub base: u64,
    pub len: u64,
}

// Helper to get current PID safely and solve E0597
fn get_current_pid() -> usize {
    let arc_clone;
    {
        let current_lock = CURRENT_TASK.lock();
        if let Some(task) = current_lock.as_ref() {
             arc_clone = task.clone();
        } else {
             panic!("Zombie");
        }
    }
    let guard = arc_clone.lock();
    guard.id
}

pub fn sys_poll(fds: usize, nfds: usize, timeout: usize) -> isize {
    let fds_ptr = fds as *mut PollFd;
    // Timeout is in milliseconds
    let _timeout_ms = timeout as i32;

    // Safety check for user pointer
    if nfds > 0 && fds_ptr.is_null() {
        return -14; // EFAULT
    }

    // Checking FDs
    if nfds > 0 {
        let slice = unsafe { core::slice::from_raw_parts_mut(fds_ptr, nfds) };
        
        // Simple polling loop with timeout simulation
        // We iterate a few times to allow interrupts to fire (UART is slow)
        let loops = if timeout == 0 { 1 } else { 100000 }; 
        
        for _ in 0..loops {
             let mut events_found = 0;
             for pollfd in slice.iter_mut() {
                // FD 0 (STDIN) Check
                if pollfd.fd == 0 {
                    if crate::drivers::console_input::has_data() || crate::drivers::console::is_data_ready() {
                        pollfd.revents |= POLLIN;
                        events_found += 1;
                    }
                }
                // FD 1/2 (STDOUT/STDERR) are always writable
                if pollfd.fd == 1 || pollfd.fd == 2 {
                    pollfd.revents |= POLLOUT;
                    events_found += 1;
                }
             }
             
             if events_found > 0 {
                 return events_found; // Return immediately if we found something
             }
             
             // Wait a bit (allow interrupts)
             if loops > 1 {
                 unsafe { core::arch::asm!("hlt"); }
             }
        }
    }
    
    // Return 0 (timeout)
    0
}

pub fn sys_writev(fd: usize, iov_ptr: usize, iovcnt: usize) -> isize {
    // Validate iovcnt
    if iovcnt > 1024 { return -22; } // EINVAL

    let iov_ptr = iov_ptr as *const IoVec;
    // Safety check
    if iov_ptr.is_null() { return -14; } // EFAULT
    
    let iovs = unsafe { core::slice::from_raw_parts(iov_ptr, iovcnt) };
    
    let mut total_written = 0;
    for iov in iovs {
        let res = sys_write(fd, iov.base as usize, iov.len as usize);
        if res < 0 {
            if total_written > 0 { return total_written; }
            return res;
        }
        total_written += res;
    }
    total_written
}



/// Syscall numbers (Linux x86_64 ABI compatible)
pub mod numbers {
    // Core I/O
    pub const SYS_READ: usize = 0;
    pub const SYS_WRITE: usize = 1;
    pub const SYS_OPEN: usize = 2;
    pub const SYS_CLOSE: usize = 3;
    pub const SYS_STAT: usize = 4;
    pub const SYS_FSTAT: usize = 5;
    pub const SYS_LSTAT: usize = 6;
    pub const SYS_POLL: usize = 7;
    pub const SYS_LSEEK: usize = 8;
    pub const SYS_MMAP: usize = 9;
    pub const SYS_BRK: usize = 12;
    pub const SYS_IOCTL: usize = 16;
    pub const SYS_WRITEV: usize = 20;
    pub const SYS_ACCESS: usize = 21;
    
    // File descriptors
    pub const SYS_DUP: usize = 32;
    pub const SYS_DUP2: usize = 33;
    pub const SYS_PIPE: usize = 22;
    
    // At syscalls
    pub const SYS_FACCESSAT: usize = 269;
    
    // Process
    pub const SYS_GETPID: usize = 39;
    pub const SYS_CLONE: usize = 56;
    pub const SYS_FORK: usize = 57;
    pub const SYS_VFORK: usize = 58;
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
    
    // Musl-required syscalls (critical for startup)
    pub const SYS_MPROTECT: usize = 10;
    pub const SYS_RT_SIGACTION: usize = 13;
    pub const SYS_RT_SIGPROCMASK: usize = 14;
    pub const SYS_ARCH_PRCTL: usize = 158;
    pub const SYS_SET_TID_ADDRESS: usize = 218;
    pub const SYS_EXIT_GROUP: usize = 231;
    
    // Directory operations
    pub const SYS_GETDENTS64: usize = 217;
}

/// Main syscall dispatcher
pub fn dispatch(nr: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    // TRACE DISABLED: User requests no serial usage
    /*
    unsafe {
         let msg = alloc::format!("[SC] {} ({}, {}, {})\r\n", nr, arg0, arg1, arg2);
         crate::early_serial_print(msg.as_bytes());
    }
    */
    
    // Sanity check: Linux x86_64 has ~450 syscalls, anything much larger is suspicious
    if nr > 500 {
        return -38; // ENOSYS
    }
    
    match nr {
        // Core I/O
        numbers::SYS_READ => sys_read(arg0, arg1, arg2),
        numbers::SYS_WRITE => sys_write(arg0, arg1, arg2),
        numbers::SYS_POLL => sys_poll(arg0, arg1, arg2),
        numbers::SYS_IOCTL => sys_ioctl(arg0, arg1, arg2),
        numbers::SYS_WRITEV => sys_writev(arg0, arg1, arg2),
        numbers::SYS_ACCESS => sys_access(arg0, arg1),
        numbers::SYS_FACCESSAT => sys_faccessat(arg0, arg1, arg2, 0), // arg3=flags (ignore for stub)
        numbers::SYS_OPEN => sys_open(arg0, arg1, arg2),
        numbers::SYS_CLOSE => sys_close(arg0),
        numbers::SYS_STAT => sys_stat(arg0, arg1),
        numbers::SYS_FSTAT => sys_fstat(arg0, arg1),
        numbers::SYS_LSEEK => sys_lseek(arg0, arg1 as i64, arg2),
        numbers::SYS_MMAP => sys_mmap(arg0, arg1, arg2),
        numbers::SYS_MUNMAP => sys_munmap(arg0, arg1),
        numbers::SYS_BRK => sys_brk(arg0),

        // Time
        numbers::SYS_NANOSLEEP => sys_nanosleep(arg0, arg1),

        
        // File descriptors
        numbers::SYS_DUP => sys_dup(arg0),
        numbers::SYS_DUP2 => sys_dup2(arg0, arg1),
        numbers::SYS_PIPE => sys_pipe(arg0),
        
        // Process
        numbers::SYS_GETPID => sys_getpid(),
        numbers::SYS_FORK => sys_fork(),
        numbers::SYS_VFORK => sys_vfork(),
        numbers::SYS_CLONE => sys_clone(arg0, arg1, arg2),
        numbers::SYS_EXECVE => sys_execve(arg0, arg1, arg2),
        numbers::SYS_EXIT => sys_exit(arg0),
        numbers::SYS_WAIT4 => sys_wait4(arg0 as i32, arg1, arg2),
        
        // Time
        numbers::SYS_GETTIMEOFDAY => sys_gettimeofday(arg0, arg1),

        numbers::SYS_CLOCK_GETTIME => sys_clock_gettime(arg0, arg1),
        
        // Misc
        numbers::SYS_UNAME => sys_uname(arg0),
        numbers::SYS_GETCWD => sys_getcwd(arg0, arg1),
        numbers::SYS_CHDIR => sys_chdir(arg0),
        numbers::SYS_GETUID => sys_getuid(),
        numbers::SYS_GETGID => sys_getgid(),
        numbers::SYS_GETEUID => sys_geteuid(),

        numbers::SYS_GETEGID => sys_getegid(),
        
        // Directory
        numbers::SYS_GETDENTS64 => sys_getdents64(arg0, arg1, arg2),
        
        // Musl-required syscalls (stubs for BusyBox startup)
        numbers::SYS_MPROTECT => {
            // mprotect(addr, len, prot) - pretend it always succeeds
            0
        }
        numbers::SYS_RT_SIGACTION => {
            // rt_sigaction(signum, act, oldact, sigsetsize) - pretend it succeeds
            0
        }
        numbers::SYS_RT_SIGPROCMASK => {
            // rt_sigprocmask(how, set, oldset, sigsetsize) - pretend it succeeds
            0
        }
        numbers::SYS_ARCH_PRCTL => {
            // arch_prctl(code, addr) - for setting FS/GS base
            // ARCH_SET_FS = 0x1002, ARCH_SET_GS = 0x1001
            let code = arg0;
            let addr = arg1;
            if code == 0x1002 {
                // Set FS base - musl uses this for TLS
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    use x86_64::registers::model_specific::FsBase;
                    FsBase::write(x86_64::VirtAddr::new(addr as u64));
                }
                0
            } else {
                log::debug!("[syscall] arch_prctl code=0x{:x} addr=0x{:x}", code, addr);
                0  // Pretend success for other codes too
            }
        }
        numbers::SYS_SET_TID_ADDRESS => {
            // set_tid_address(tidptr) - return current TID
            // We return PID as TID for single-threaded process
            sys_getpid()
        }
        numbers::SYS_EXIT_GROUP => {
            // exit_group(status) - exit all threads
            sys_exit(arg0)
        }
        
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
    
    // SCREEN DEBUG: [O: filename]
    unsafe {
        crate::video::console_print_char(b'[');
        crate::video::console_print_char(b'O');
        crate::video::console_print_char(b':');
        for &b in filename.as_bytes() {
            crate::video::console_print_char(b);
        }
        crate::video::console_print_char(b']');
    }

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
    // Poll UEFI input (Hack for Hyper-V)
    if fd == 0 {
        crate::drivers::uefi_input::poll();
    }

    // Hardcoded STDIN for now (Keyboard)
    if fd == 0 {
        if count == 0 { return 0; }
        
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, count) };
        let mut nread = 0;
        
        // Blocking Read Loop
        loop {
            // FORCE ENABLE INTERRUPTS (`sti`)
            // The `syscall` instruction disables interrupts (SFMASK).
            // But UEFI polling or legacy USB emulation might require timer interrupts/SMM to process USB.
            unsafe { core::arch::asm!("sti"); }
            
            // UEFI Poll: REQUIRED for bare metal USB keyboards if PS/2 is dead
            // We can do this because we haven't exited boot services!
            crate::drivers::uefi_input::poll();
            
            let mut got_char = false;
            
            // Only read from the Global Input Queue (fed by Interrupts OR UEFI poll)
            if let Some(c) = crate::drivers::console_input::pop_char() {
                buf[nread] = c;
                nread += 1;
                got_char = true;
                // Debug echo to screen
                // unsafe { crate::video::console_print_char(c); }
            }
            // SERIAL POLLING REMOVED (Fix '????' spam caused by floating bus 0xFF)
            
            if got_char {
                // If we filled the buffer, return
                if nread >= count {
                    // Disable Interrupts again before returning (Syscall ABI usually assumes IF=0 on exit path, though sysret sets it back from R11)
                    // Actually, sysret will load R11 into RFLAGS. R11 came from old RFLAGS (IF=1).
                    // So we don't strictly need to CLI, but it's safer for kernel state consistency.
                    unsafe { core::arch::asm!("cli"); }
                    return nread as isize;
                }
                // If we have data but buffer not full, we COULD return,
                // or check for more. Standard TTY usually returns line-buffered or immediate.
                // For raw blocking read, return what we have (short read).
                // But let's check one more time just in case of pasted input.
                if !crate::drivers::console_input::has_data() {
                     unsafe { core::arch::asm!("cli"); }
                     return nread as isize;
                }
            } else {
                // No data yet.
                if nread > 0 {
                    unsafe { core::arch::asm!("cli"); }
                    return nread as isize;
                }
                
                // Wait for Interrupts OR Poll Delay
                crate::sched::yield_now();
            }
        }
    }

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
    // Special handling for stdout/stderr
    if fd == 1 || fd == 2 {
        unsafe {
            let slice = core::slice::from_raw_parts(buf_ptr as *const u8, count);
            
            // DEBUG: Send to Serial for confirmation
            crate::early_serial_print(b"[STDOUT] ");
            crate::early_serial_print(slice);
            crate::early_serial_print(b"\r\n");
            
            // Write to framebuffer console (Safe for real hardware)
            for &byte in slice {
                crate::video::console_print_char(byte);
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
    // SCREEN DEBUG: [EXIT: code]
    unsafe {
        crate::video::console_print_char(b'[');
        crate::video::console_print_char(b'E');
        crate::video::console_print_char(b'X');
        crate::video::console_print_char(b'I');
        crate::video::console_print_char(b'T');
        crate::video::console_print_char(b':');
        let c = code as u8;
        if code == 0 {
             crate::video::console_print_char(b'0');
        } else {
             crate::video::console_print_char(if c < 10 { b'0' + c } else { b'X' }); 
        }
        crate::video::console_print_char(b']');
        crate::video::console_print_char(b'\n');
    }

    log::info!("[syscall::exit] Process exited with code {}", code);
    
    // Update Legacy Task State
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let mut task = task_arc.lock();
        task.state = crate::sched::task::TaskState::Terminated;
        
        // Update CORE Scheduler State
        if let Some(mut sched_lock) = crate::globals::SCHEDULER.lock().as_mut() {
             if let Some(proc) = sched_lock.get_process_mut(task.id as u64) {
                 use aether_core::scheduler::ProcessState;
                 proc.state = ProcessState::Terminated;
                 log::info!("[syscall::exit] Core Process {} Terminated", task.id);
             }
        }
    }
    drop(current_lock); // Release lock before yielding
    
    // Yield forever (Scheduler will switch away and never switch back)
    loop {
        crate::sched::yield_now();
    }
}

// ============================================================================
// Extended Syscalls (Phase 14)
// ============================================================================

/// Program break management (heap allocation)
/// For now, we use a simple linear allocator
static mut PROGRAM_BREAK: usize = 0x40000000; // Start at 1GB to avoid collisions

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
    // TRACE REMOVED
    
    // Simple anonymous mapping at requested address
    if addr == 0 {
        // Kernel chooses address
        unsafe {
            let new_addr = PROGRAM_BREAK;
            // Align check (should be aligned)
            
            let aligned_len = (length + 4095) & !4095;
            PROGRAM_BREAK += aligned_len;
            
            crate::mm::paging::make_user_accessible(new_addr as u64, aligned_len as u64);
            log::debug!("[syscall::mmap] Mapped {} bytes at 0x{:x}", aligned_len, new_addr);
            
            // CRITICAL FIX: Zero the memory! 
            // make_user_accessible likely reuses Identity Mapped pages (dirty RAM).
            // Musl expects zeroed memory for BSS/Heap.
            // We use the direct address since it's identity mapped and user-accessible.
            core::ptr::write_bytes(new_addr as *mut u8, 0, aligned_len);

            // TRACE REMOVED
            
            return new_addr as isize;
        }
    }
    
    // Fixed address mapping
    let aligned_len = (length + 4095) & !4095;
    crate::mm::paging::make_user_accessible(addr as u64, aligned_len as u64);
    log::debug!("[syscall::mmap] Mapped {} bytes at 0x{:x} (fixed)", aligned_len, addr);
    
    // Zero Fixed Mapping too
    unsafe { core::ptr::write_bytes(addr as *mut u8, 0, aligned_len); }
    
    addr as isize
}

// ============================================================================
// File Syscalls (Phase 14 - POSIX)
// ============================================================================

fn sys_access(path: usize, _mode: usize) -> isize {
    // Check if file exists
    let filename = unsafe { get_user_string(path, 0) };
    if filename.is_none() { return -14; } // EFAULT
    let filename = filename.unwrap();
    
    // For now, if file exists, assume ALL permissions (including X_OK)
    match fs::open(&filename, 0) {
        Ok(_) => 0, // Success
        Err(_) => -2, // ENOENT
    }
}

fn sys_faccessat(_dirfd: usize, path: usize, _mode: usize, _flags: usize) -> isize {
    // Simplified stub: ignore dirfd/flags, behaves like access
    sys_access(path, _mode)
}

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

fn sys_stat(path: usize, statbuf: usize) -> isize {
    // 1. Get Path
    let filename = unsafe { get_user_string(path, 0) };
    if filename.is_none() { return -14; } // EFAULT
    let filename = filename.unwrap();
    
    // 2. Resolve Path
    // Pass 0 flags for simple open/lookup
    match fs::open(&filename, 0) {
        Ok(inode) => {
             // 3. Get Metadata
             let metadata = inode.metadata();
             
             // 4. Fill statbuf
             if statbuf != 0 {
                 unsafe {
                     let buf = statbuf as *mut u8;
                     // Only fill what's needed for simple shell
                     // Layout (x86_64):
                     // 0..8: st_dev
                     // 8..16: st_ino 
                     // 16..24: st_nlink
                     // 24..28: st_mode (u32)
                     
                     // Helper
                     let write_u64 = |offset, val| {
                         core::ptr::write_unaligned(buf.add(offset) as *mut u64, val);
                     };
                     let write_u32 = |offset, val| {
                         core::ptr::write_unaligned(buf.add(offset) as *mut u32, val);
                     };

                     // S_IFREG = 0o100000 (0x8000)
                     // S_IFDIR = 0o040000 (0x4000)
                     let s_ifreg = 0o100000;
                     let s_ifdir = 0o040000;
                     
                     let mode_val = metadata.mode.0;
                     
                     // Heuristic: If executable, assume regular file. If directory, assume directory.
                     // But metadata.file_type tells us!
                     let file_type_flag = match metadata.file_type {
                         crate::fs::vfs::FileType::Directory => s_ifdir,
                         crate::fs::vfs::FileType::File => s_ifreg,
                         _ => 0, // Device?
                     };
                     
                     let final_mode = mode_val | file_type_flag;
                     
                     write_u64(8, 1); // st_ino (fake)
                     write_u32(24, final_mode); // st_mode with Type
                     write_u32(28, 0); // st_uid (root)
                     write_u32(32, 0); // st_gid (root)
                     write_u64(48, metadata.size); // st_size
                 }
             }
             0
        },
        Err(_) => -2, // ENOENT
    }
}

fn sys_fstat(fd: usize, statbuf: usize) -> isize {
    let current_lock = CURRENT_TASK.lock();
    if let Some(task_arc) = current_lock.as_ref() {
        let task = task_arc.lock();
        if let Some(Some(file)) = task.fd_table.get(fd) {
             let metadata = file.inode.metadata();
             
             if statbuf != 0 {
                 unsafe {
                     let buf = statbuf as *mut u8;
                     let write_u64 = |offset, val| {
                         core::ptr::write_unaligned(buf.add(offset) as *mut u64, val);
                     };
                     let write_u32 = |offset, val| {
                         core::ptr::write_unaligned(buf.add(offset) as *mut u32, val);
                     };
                     
                     let s_ifreg = 0o100000;
                     let s_ifdir = 0o040000;
                     let file_type_flag = match metadata.file_type {
                         crate::fs::vfs::FileType::Directory => s_ifdir,
                         crate::fs::vfs::FileType::File => s_ifreg,
                         _ => 0,
                     };
                     
                     write_u64(8, 1); // st_ino
                     write_u32(24, metadata.mode.0 | file_type_flag); // st_mode
                     write_u32(28, 0); // st_uid
                     write_u32(32, 0); // st_gid
                     write_u64(48, metadata.size); // st_size
                 }
             }
             return 0;
        }
    }
    -9 // EBADF
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

fn sys_nanosleep(req: usize, _rem: usize) -> isize {
    // struct timespec { time_t tv_sec; long tv_nsec; };
    // We assume x86_64, so tv_sec is i64 (8 bytes), tv_nsec is i64 (8 bytes)
    let ptr = req as *const u64; // [sec, nsec]
    if ptr.is_null() { return -14; } // EFAULT
    
    let sec = unsafe { *ptr };
    let nsec = unsafe { *ptr.add(1) };
    
    // Convert to ms
    let ms = (sec * 1000) + (nsec / 1_000_000);
    
    // Call scheduler sleep
    log::trace!("[syscall::nanosleep] Sleeping for {} ms", ms);
    crate::sched::sleep(ms);
    0
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

// Linux dirent64 structure
// struct linux_dirent64 {
//    ino64_t        d_ino;    /* 64-bit inode number */
//    off64_t        d_off;    /* 64-bit offset to next structure */
//    unsigned short d_reclen; /* Size of this dirent */
//    unsigned char  d_type;   /* File type */
//    char           d_name[]; /* Filename (null-terminated) */
// };
#[repr(C, packed)]
#[repr(C, packed)]
struct LinuxDirent64Header {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

fn sys_getdents64(fd: usize, dirp: usize, count: usize) -> isize {
    crate::early_serial_print(b"[Syscall] getdents64 called\r\n");
    let current_lock = CURRENT_TASK.lock();
    let task_arc = match current_lock.as_ref() {
        Some(t) => t.clone(),
        None => return -9, // EBADF
    };
    drop(current_lock); // Drop global lock to avoid deadlock if file ops block/alloc
    
    let mut task = task_arc.lock();
    if let Some(file_opt) = task.fd_table.get_mut(fd) {
        if let Some(file) = file_opt {
            // Use abstract VFS read_dir
            match file.inode.read_dir() {
                Ok(entries_raw) => {
                    let mut entries = Vec::new();
                    // Synthesize . and ..
                    // In a real FS, read_dir might return them, or not.
                    // RamFS read_dir does NOT return . and ..
                    entries.push((String::from("."), 1));
                    entries.push((String::from(".."), 1));
                    
                    entries.extend(entries_raw);
                    
                    let start_index = file.offset as usize;
                    let mut current_index = start_index;
                    
                    // Skip entries we already read
                    // Optimization: We could have read_dir take an offset, but inefficient for simple RamFS is fine
                    if start_index >= entries.len() {
                         return 0; // EOF
                    }
                    
                    let mut output_ptr = dirp as *mut u8;
                    let mut remaining = count;
                    let mut bytes_written = 0;
                    
                    for i in start_index..entries.len() {
                         let (name, _ino) = &entries[i];
                         let name_bytes = name.as_bytes();
                         let name_len = name_bytes.len();
                         let reclen = (core::mem::size_of::<LinuxDirent64Header>() + name_len + 1 + 7) & !7; // Align to 8
                         
                         if reclen > remaining {
                             break;
                         }
                         
                         unsafe {
                             let header = output_ptr as *mut LinuxDirent64Header;
                             (*header).d_ino = 1; 
                             (*header).d_off = (current_index + 1) as u64; 
                             (*header).d_reclen = reclen as u16;
                             (*header).d_type = 4; // DT_DIR (Simplification: everything is a dir? No.)
                             // Ideally we request type from VFS or stat it.
                             // For now, assume DIR if . or .., else UNKNOWN (0)
                             if name == "." || name == ".." {
                                 (*header).d_type = 4;
                             } else {
                                 (*header).d_type = 0; // DT_UNKNOWN (ls will stat to check)
                             }
                             
                             let name_ptr = output_ptr.add(core::mem::size_of::<LinuxDirent64Header>());
                             core::ptr::copy_nonoverlapping(name_bytes.as_ptr(), name_ptr, name_len);
                             *name_ptr.add(name_len) = 0; // Null terminator
                             
                             output_ptr = output_ptr.add(reclen);
                             remaining -= reclen;
                             bytes_written += reclen;
                         }
                         current_index += 1;
                    }
                    
                    file.offset = current_index as u64;
                    return bytes_written as isize;
                },
                Err(_) => return -20, // ENOTDIR
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
// Process Syscalls - Fork/Exec/Wait
// ============================================================================

/// Fork - Create child process
fn sys_fork() -> isize {
    sys_fork_impl()
}

fn sys_vfork() -> isize {
    // vfork MUST block parent until child execs or exits.
    
    log::info!("[syscall::vfork] Creating vfork child (Blocking Parent)...");
    
    // 1. Get Parent Info from Scheduler (Real Stack/State)
    let parent_id: u64;
    let parent_stack: Vec<u8>;
    let parent_cr3: u64;
    
    {
        // DEBUG: Check Scheduler Address
        let sched_addr = &*crate::globals::SCHEDULER as *const _ as u64;
        
        let mut sched_lock = crate::globals::SCHEDULER.lock();
        if sched_lock.is_none() {
            // Read raw memory to see if it's 0 or garbage
            let raw_val = unsafe { *(sched_addr as *const u64) };
            panic!("[syscall::vfork] Scheduler GONE! Addr: 0x{:x} RawVal: 0x{:x}", sched_addr, raw_val);
        }
        let sched = sched_lock.as_mut().unwrap();
        
        // Save current state FIRST
        if let Some(pid) = sched.current_pid {
            parent_id = pid as u64;
        } else {
             // Fallback for PID 0/Init
             parent_id = 0;
        }
        
        let parent_proc = sched.get_process_mut(parent_id).expect("Zombie Process");
        parent_stack = parent_proc.stack.clone(); 
        parent_cr3 = parent_proc.cr3;
    }

    
    // 2. Capture Registers
    let current_rsp: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) current_rsp); }
    
    // 3. Create Child Task (Metadata)
    let stack_len = parent_stack.len();
    let mut child_task = crate::sched::task::Task::new(stack_len);
    
    child_task.parent_id = parent_id as usize;
    child_task.stack = parent_stack; 
    child_task.saved_rsp = current_rsp;
    child_task.saved_rip = 0;
    child_task.cr3 = parent_cr3;
    
    // Copy FDs
    {
         let current_lock = CURRENT_TASK.lock();
         if let Some(parent_task_arc) = current_lock.as_ref() {
             let parent_t = parent_task_arc.lock();
             child_task.fd_table = parent_t.fd_table.clone();
         }
    }
    
    // 4. Spawn
    let child_pid = crate::sched::queue::spawn_task(child_task);

    // Identity check - Logic similar to fork return trick
    // BUT since we manually constructed child_task with 'current_rsp', 
    // when Child runs, it pops 'current_rsp'. 
    // It returns to... HERE.
    // So we need to distinguish.
    
    // We check our PID.
    let pid = get_current_pid();
    
    if pid == child_pid {
        // Child
        return 0;
    } else {
        // Parent - BLOCK
        log::info!("[syscall::vfork] Parent {} yielding for Child {}", pid, child_pid);
        for _ in 0..20 {
             crate::sched::yield_now();
        }
        return child_pid as isize;
    }
}

fn sys_clone(_flags: usize, _stack: usize, _parent_tid: usize) -> isize {
    log::info!("[syscall::clone] Using fork implementation");
    sys_fork_impl()
}

fn sys_fork_impl() -> isize {
    log::info!("[syscall::fork] Forking...");
    
    // 1. Get Parent Info from Scheduler (Real Stack/State)
    let parent_id: u64;
    let parent_stack: Vec<u8>;
    let parent_cr3: u64;
    
    {
        // 1. Get Parent Info from Scheduler
        // We expect the scheduler to be initialized and the current process to be valid.
        let mut sched_lock = crate::globals::SCHEDULER.lock();
        let sched = sched_lock.as_mut().expect("Scheduler not initialized!");
        
        // Save current state FIRST
        if let Some(pid) = sched.current_pid {
            parent_id = pid as u64;
        } else {
             // If we are here, it means we are in Init but `current_pid` is None?
             // But we just set `current_pid = Some(1)` in main!
             // So this should be unreachable unless context switch cleared it (Idle?).
             // If Idle calls fork? Impossible.
             panic!("[syscall::fork] Current PID is None! Init process ghosting?");
        }
        
        let parent_proc = sched.get_process_mut(parent_id).expect("Zombie Process or Invalid PID");
        parent_stack = parent_proc.stack.clone(); 
        parent_cr3 = parent_proc.cr3;
    }
    
    // 2. Clone Page Table (CR3) - DEEP COPY Logic via mm::paging
    let new_cr3 = crate::mm::paging::clone_process_page_table(parent_cr3);
    log::info!("[syscall::fork] Cloned Page Table: Old=0x{:x} New=0x{:x}", parent_cr3, new_cr3);

    // 3. Capture Registers
    let current_rsp: u64;
    unsafe { core::arch::asm!("mov {}, rsp", out(reg) current_rsp); }
    
    // 4. Create Child Task (Metadata)
    let stack_len = parent_stack.len();
    let mut child_task = crate::sched::task::Task::new(stack_len);
    
    child_task.parent_id = parent_id as usize;
    child_task.stack = parent_stack; 
    child_task.saved_rsp = current_rsp;
    child_task.saved_rip = 0;
    child_task.cr3 = new_cr3; // Use NEW CR3 
    
    // Copy FDs
    {
         let current_lock = CURRENT_TASK.lock();
         if let Some(parent_task_arc) = current_lock.as_ref() {
             let parent_t = parent_task_arc.lock();
             child_task.fd_table = parent_t.fd_table.clone();
         }
    }
    
    // 4. Spawn
    let child_pid = crate::sched::queue::spawn_task(child_task);

    // Identity check
    let pid = get_current_pid(); // usize
    
    if pid == child_pid as usize {
        0
    } else {
        child_pid as isize
    }
}

fn sys_execve(pathname: usize, argv: usize, envp: usize) -> isize {
    // SCREEN DEBUG: [EXEC]
    unsafe {
        crate::video::console_print_char(b'[');
        crate::video::console_print_char(b'E');
        crate::video::console_print_char(b'X');
        crate::video::console_print_char(b'E');
        crate::video::console_print_char(b'C');
        crate::video::console_print_char(b']');
    }

    // Get pathname string
    let path = unsafe { get_user_string(pathname, 0) };
    if path.is_none() {
        log::warn!("[syscall::execve] Invalid pathname");
        return -14; // EFAULT
    }
    let path_str = path.unwrap();
    log::info!("[syscall::execve] Path: {}", path_str);
    
    // Open file
    let inode = match fs::open(&path_str, 0) {
        Ok(inode) => inode,
        Err(_) => {
            log::warn!("[syscall::execve] File not found: {}", path_str);
            return -2; // ENOENT
        }
    };
    
    // Read file header to check type
    let mut buffer = alloc::vec![0u8; 4096]; // Read header + some data
    let len = inode.read_at(0, &mut buffer);
    if len < 64 {
        log::warn!("[syscall::execve] File too small");
        return -8; // ENOEXEC
    }
    
    let buffer_slice = &buffer[..len];
    let header = unsafe { *(buffer_slice.as_ptr() as *const elf::Elf64Header) };
    
    // Determine Main Load Base
    // With CR3 Isolation, we can theoretically load at 0x400000 always!
    // But to be safe and compatible with our PID-relocation hack (which is still good for debugging),
    // we can stick to relocation OR switch to fixed base.
    // Let's TRY Fixed Base (0x400000) now that we have isolation!
    // "ET_DYN" can define 0. "ET_EXEC" defines fixed.
    // If we use isolation, we don't need relocation.
    let main_base = if header.e_type == 3 { 0x00400000 } else { 0 };
    
    // ISOLATION STEP: Create New Address Space
    // Pass 0 to create a fresh User Space (Based on Kernel Boot, Empty User Mappings)
    let new_cr3 = crate::mm::paging::clone_process_page_table(0);
    
    log::info!("[syscall::execve] Created new Address Space CR3=0x{:x}. Switching...", new_cr3);
    
    // CRITICAL FIX: Switch to new CR3 to load ELF!
    // Otherwise we load into OLD address space, but jump to NEW address space (which is empty).
    unsafe {
        use x86_64::registers::control::Cr3;
        use x86_64::structures::paging::PhysFrame;
        use x86_64::PhysAddr;
        
        let (_, flags) = Cr3::read();
        Cr3::write(PhysFrame::containing_address(PhysAddr::new(new_cr3)), flags);
    }
    
    // Load Main ELF (into new space)
    let loaded = match elf::load_elf(buffer_slice, main_base) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("[syscall::execve] ELF load error: {}", e);
            // Restore old CR3? Or just die.
            // Current task is corrupted/half-switched.
            // We should terminate.
            return -8; // ENOEXEC
        }
    };
    
    // Prepare Auxv
    let mut auxv = Vec::new();
    let entry_point;
    
    // Check for Interpreter
    if let Some(interp_path) = loaded.interp {
        log::info!("[syscall::execve] Interpreter requested: {}", interp_path);
        
        // Open Interpreter
        // Note: fs::open uses VFS (RamFS), which is in Kernel Heap.
        // Kernel Heap is mapped in New CR3 (Direct Map).
        // So this works!
        let interp_inode = match fs::open(&interp_path, 0) {
            Ok(inode) => inode,
            Err(_) => {
                log::warn!("[syscall::execve] Interpreter not found: {}", interp_path);
                return -2; // ENOENT
            }
        };
        
        let mut interp_buf = alloc::vec![0u8; 256 * 1024]; 
        let interp_len = interp_inode.read_at(0, &mut interp_buf);
        
        // Load Interpreter
        let interp_base = 0x7ffff7dd5000;
        let interp_loaded = match elf::load_elf(&interp_buf[..interp_len], interp_base) {
             Ok(l) => l,
             Err(e) => {
                 log::warn!("[syscall::execve] Interpreter load error: {}", e);
                 return -8; // ENOEXEC
             }
        };
        
        entry_point = interp_loaded.entry_point;
        
        auxv.push(elf::AuxvEntry { key: elf::AT_PHDR, val: loaded.phdr_vaddr });
        auxv.push(elf::AuxvEntry { key: elf::AT_PHENT, val: loaded.phentsize as u64 });
        auxv.push(elf::AuxvEntry { key: elf::AT_PHNUM, val: loaded.phnum as u64 });
        auxv.push(elf::AuxvEntry { key: elf::AT_ENTRY, val: loaded.entry_point });
        auxv.push(elf::AuxvEntry { key: elf::AT_BASE, val: interp_base });
    } else {
        // Static Executable
        log::info!("[syscall::execve] Static executable");
        entry_point = loaded.entry_point;
        
        auxv.push(elf::AuxvEntry { key: elf::AT_PHDR, val: loaded.phdr_vaddr });
        auxv.push(elf::AuxvEntry { key: elf::AT_PHENT, val: loaded.phentsize as u64 });
        auxv.push(elf::AuxvEntry { key: elf::AT_PHNUM, val: loaded.phnum as u64 });
        auxv.push(elf::AuxvEntry { key: elf::AT_PAGESZ, val: 4096 });
    }
    
    // Parse argv - DEEP COPY into Kernel Heap
    let mut argv_vec: Vec<Vec<u8>> = Vec::new(); // Changed from Vec<&[u8]>
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
                // Deep Copy: Create owned Vec<u8>
                let mut arg_content = alloc::vec![0u8; len];
                core::ptr::copy_nonoverlapping(arg_ptr, arg_content.as_mut_ptr(), len);
                argv_vec.push(arg_content);
                
                ptr = ptr.add(1);
            }
        }
    }
    
    // Parse envp (simplified) - DEEP COPY
    let envp_vec: Vec<Vec<u8>> = Vec::new(); // Changed from Vec<&[u8]>
    
    // Set up new stack - Standard User Stack Top
    let stack_top = 0x7FFFFF000000u64;
    let stack_size = 128 * 1024; // 128KB stack
    crate::mm::paging::make_user_accessible(stack_top - stack_size, stack_size);
    
    // Set up stack with argv/envp/auxv
    // Set up stack with argv/envp/auxv
    // Convert Vec<Vec<u8>> back to &[&[u8]] for setup_user_stack logic?
    // Or just refactor setup_user_stack?
    // Easiest is to convert here temporarily (slices point to Kernel Heap Vecs)
    let argv_slices: Vec<&[u8]> = argv_vec.iter().map(|v| v.as_slice()).collect();
    let envp_slices: Vec<&[u8]> = envp_vec.iter().map(|v| v.as_slice()).collect();
    
    let user_sp = elf::setup_user_stack(stack_top, &argv_slices, &envp_slices, &auxv);
    
    log::info!("[syscall::execve] Stack at 0x{:x}, entry 0x{:x}", user_sp, entry_point);
    
    // UPDATE Process Task Structure with new CR3
    {
        let current_lock = CURRENT_TASK.lock();
        if let Some(task) = current_lock.as_ref() {
            let mut t = task.lock();
            t.cr3 = new_cr3;
        }
    }
    
    // CRITICAL: Update Scheduler Process CR3
    // The Scheduler is the source of truth for Context Switching.
    // If we don't update this, the next Timer Interrupt will revert CR3 to 0.
    {
         let mut sched_lock = crate::globals::SCHEDULER.lock();
         if let Some(sched) = sched_lock.as_mut() {
             if let Some(pid) = sched.current_pid {
                  if let Some(proc) = sched.get_process_mut(pid) {
                      proc.cr3 = new_cr3;
                      log::info!("[syscall::execve] Updated Scheduler CR3 for PID {}", pid);
                  }
             }
         }
    }
    
    // Jump to new program
    // Jump to new program
    crate::early_serial_print(b"[Exec] Success! Jumping to User Mode...\r\n");
    // Force Screen Output
    unsafe {
        for &b in b"[Exec] Success! Jumping to User Mode...\n" {
            crate::video::console_print_char(b);
        }
    }
    
    #[cfg(target_arch = "x86_64")]
    unsafe {
        crate::arch::x86_64::enter_usermode(entry_point, user_sp);
    }
    
    #[cfg(target_arch = "aarch64")]
    unsafe {
        crate::arch::aarch64::enter_usermode(entry_point, user_sp);
    }
    
    -1
}

fn sys_wait4(pid: i32, wstatus: usize, _options: usize) -> isize {
    log::info!("[syscall::wait4] Waiting for PID {}...", pid);
    
    if pid <= 0 {
         // Wait for any child
         crate::sched::yield_now(); // Yield to let children run
         // Real shell needs Any Child wait logic.
         // For simplistic "sh", it usually waits for specific PID it just forked.
         // If pid is -1, we loop a bit then return.
         for _ in 0..100 { crate::sched::yield_now(); }
         return -10; // ECHILD? 
    }
    
    let target_pid = pid as u64;
    
    // Polling Loop: Wait until target process is TERMINATED
    // In a real OS, we would sleep on a Condition Variable / WaitQueue.
    // Here we spin-yield.
    let mut ticks = 0;
    loop {
        let mut finished = false;
        
        // Scope the lock
        {
             let mut sched_lock = crate::globals::SCHEDULER.lock();
             if let Some(sched) = sched_lock.as_mut() {
                 if let Some(proc) = sched.get_process_mut(target_pid) {
                     if proc.state == aether_core::scheduler::ProcessState::Terminated {
                         finished = true;
                     }
                 } else {
                     // Process GONE? Maybe cleaned up?
                     // Consider it finished.
                     finished = true;
                     log::warn!("[syscall::wait4] PID {} is gone!", target_pid);
                 }
             }
        }
        
        if finished {
             log::info!("[syscall::wait4] PID {} Terminated. Returning.", target_pid);
             break;
        }
        
        // Yield to allow Child to run
        crate::sched::yield_now();
        ticks += 1;
        
        if ticks % 1000 == 0 {
             // log::trace!("[syscall::wait4] Still waiting for {} ({})", target_pid, ticks);
        }
    }

    // Return fake success
    if wstatus != 0 {
         unsafe { *(wstatus as *mut i32) = 0; } // Status 0
    }
    
    pid as isize
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

fn sys_chdir(path: usize) -> isize {
    // Stub - only allow "/"
    let filename = unsafe { get_user_string(path, 0) };
    if let Some(name) = filename {
         if name == "/" {
             return 0;
         }
    }
    log::debug!("[syscall::chdir] Stub - rejecting");
    -2 // ENOENT
}

fn sys_getuid() -> isize { 0 }   // root
fn sys_getgid() -> isize { 0 }   // root
fn sys_geteuid() -> isize { 0 }  // root
fn sys_getegid() -> isize { 0 }  // root
