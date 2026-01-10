//! DevFS - /dev Virtual Filesystem
//!
//! Implements standard device nodes:
//! - /dev/null: discards all writes, returns EOF on read
//! - /dev/zero: returns zeros on read, discards writes
//! - /dev/urandom: returns random bytes
//! - /dev/tty: console device (stdin/stdout proxy)

use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use spin::Mutex;

use crate::fs::vfs::{Inode, Metadata, FileType, FileMode, FsError, FileSystem};

// ============================================================================
// /dev/null
// ============================================================================

pub struct DevNull;

impl Inode for DevNull {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> usize {
        0 // EOF
    }
    
    fn write_at(&self, _offset: u64, buf: &[u8]) -> usize {
        buf.len() // Accept and discard
    }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 0,
            mode: FileMode(0o666),
            file_type: FileType::Device,
        }
    }
}

// ============================================================================
// /dev/zero
// ============================================================================

pub struct DevZero;

impl Inode for DevZero {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> usize {
        buf.fill(0);
        buf.len()
    }
    
    fn write_at(&self, _offset: u64, buf: &[u8]) -> usize {
        buf.len() // Accept and discard
    }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 0,
            mode: FileMode(0o666),
            file_type: FileType::Device,
        }
    }
}

// ============================================================================
// /dev/urandom - Simple PRNG (not cryptographically secure!)
// ============================================================================

pub struct DevUrandom {
    state: Mutex<u64>,
}

impl DevUrandom {
    pub fn new() -> Self {
        // Seed with something. In real kernel, use TSC or RNG hardware
        Self { state: Mutex::new(0x853c49e6748fea9b) }
    }
    
    fn next_u64(&self) -> u64 {
        let mut state = self.state.lock();
        // xorshift64 PRNG
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }
}

impl Inode for DevUrandom {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> usize {
        let mut i = 0;
        while i < buf.len() {
            let rand = self.next_u64();
            let bytes = rand.to_le_bytes();
            for &b in &bytes {
                if i >= buf.len() { break; }
                buf[i] = b;
                i += 1;
            }
        }
        buf.len()
    }
    
    fn write_at(&self, _offset: u64, buf: &[u8]) -> usize {
        // Mix entropy into state
        let mut state = self.state.lock();
        for &b in buf {
            *state = state.wrapping_add(b as u64).wrapping_mul(0x5851f42d4c957f2d);
        }
        buf.len()
    }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 0,
            mode: FileMode(0o666),
            file_type: FileType::Device,
        }
    }
}

// ============================================================================
// /dev/tty - Console device (proxy to kernel serial/console)
// ============================================================================

pub struct DevTty;

impl Inode for DevTty {
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> usize {
        if buf.is_empty() { return 0; }
        
        // Simple blocking read for TTY
        // We wait for at least one character
        let mut nread = 0;
        
        loop {
             // 1. Try Keyboard Buffer
             if let Some(c) = crate::drivers::console_input::pop_char() {
                 buf[nread] = c as u8;
                 nread += 1;
             } 
             // 2. Try Serial Buffer/Hardware
             else if let Some(c) = crate::drivers::console::read_serial() {
                 buf[nread] = c;
                 nread += 1;
             }
             
             // If we have data, return it immediately (don't wait for full buffer)
             // This gives interactive feel
             if nread > 0 {
                 return nread;
             }
             
             // No data yet - Wait/Yield
             // TODO: Use proper wait queue
             #[cfg(target_arch = "x86_64")]
             unsafe { core::arch::asm!("hlt"); }
             
             // Poll UEFI (hack for Hyper-V if interrupts are tricky)
             crate::drivers::uefi_input::poll();
        }
    }
    
    fn write_at(&self, _offset: u64, buf: &[u8]) -> usize {
        // Write to serial console
        for &b in buf {
            #[cfg(target_arch = "x86_64")]
            unsafe {
                // Write to COM1 (0x3F8)
                x86_64::instructions::port::Port::<u8>::new(0x3F8).write(b);
            }
            #[cfg(target_arch = "aarch64")]
            {
                // Would write to UART
                let _ = b;
            }
        }
        buf.len()
    }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 0,
            mode: FileMode(0o666),
            file_type: FileType::Device,
        }
    }
}

// ============================================================================
// DevFS Root Directory
// ============================================================================

pub struct DevFsRoot {
    null: Arc<DevNull>,
    zero: Arc<DevZero>,
    urandom: Arc<DevUrandom>,
    tty: Arc<DevTty>,
}

impl DevFsRoot {
    pub fn new() -> Self {
        Self {
            null: Arc::new(DevNull),
            zero: Arc::new(DevZero),
            urandom: Arc::new(DevUrandom::new()),
            tty: Arc::new(DevTty),
        }
    }
}

impl Inode for DevFsRoot {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> usize { 0 }
    fn write_at(&self, _offset: u64, _buf: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 0,
            mode: FileMode(0o755),
            file_type: FileType::Directory,
        }
    }
    
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        Ok(alloc::vec![
            (String::from("null"), 1),
            (String::from("zero"), 2),
            (String::from("urandom"), 3),
            (String::from("tty"), 4),
        ])
    }
    
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        match name {
            "null" => Ok(self.null.clone()),
            "zero" => Ok(self.zero.clone()),
            "urandom" => Ok(self.urandom.clone()),
            "random" => Ok(self.urandom.clone()), // Alias
            "tty" => Ok(self.tty.clone()),
            _ => Err(FsError::NotFound),
        }
    }
}

// ============================================================================
// DevFS FileSystem
// ============================================================================

pub struct DevFs {
    root: Arc<DevFsRoot>,
}

impl DevFs {
    pub fn new() -> Self {
        Self { root: Arc::new(DevFsRoot::new()) }
    }
}

impl FileSystem for DevFs {
    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }
}
