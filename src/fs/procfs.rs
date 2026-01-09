//! ProcFS - /proc Virtual Filesystem
//!
//! Implements process and kernel information:
//! - /proc/self → link to current process
//! - /proc/[pid]/stat → process status
//! - /proc/[pid]/maps → memory mappings
//! - /proc/meminfo → memory info
//! - /proc/cpuinfo → CPU info

use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use spin::Lazy;

use crate::fs::vfs::{Inode, Metadata, FileType, FileMode, FsError, FileSystem};
use crate::sched::queue::{CURRENT_TASK, ALL_TASKS};

// ============================================================================
// /proc/self/stat - Process status
// ============================================================================

pub struct ProcStat {
    pid: usize,
}

impl ProcStat {
    pub fn new(pid: usize) -> Self {
        Self { pid }
    }
    
    pub fn for_current() -> Self {
        let pid = CURRENT_TASK.lock()
            .as_ref()
            .map(|t| t.lock().id)
            .unwrap_or(1);
        Self { pid }
    }
}

impl Inode for ProcStat {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        // Format: pid (name) state ppid ...
        let content = format!(
            "{} (init) S {} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 0\n",
            self.pid, 
            0 // parent pid
        );
        
        let bytes = content.as_bytes();
        if offset as usize >= bytes.len() {
            return 0;
        }
        
        let remaining = &bytes[offset as usize..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        to_copy
    }
    
    fn write_at(&self, _offset: u64, _buf: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 100,
            mode: FileMode(0o444),
            file_type: FileType::File,
        }
    }
}

// ============================================================================
// /proc/self/maps - Memory mappings (stub)
// ============================================================================

pub struct ProcMaps;

impl Inode for ProcMaps {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        // Simplified memory map
        let content = "00400000-00500000 r-xp 00000000 00:00 0 [text]\n\
                      7fffff000000-7fffffffffff rw-p 00000000 00:00 0 [stack]\n";
        
        let bytes = content.as_bytes();
        if offset as usize >= bytes.len() {
            return 0;
        }
        
        let remaining = &bytes[offset as usize..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        to_copy
    }
    
    fn write_at(&self, _offset: u64, _buf: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata {
            size: 200,
            mode: FileMode(0o444),
            file_type: FileType::File,
        }
    }
}

// ============================================================================
// /proc/meminfo - Memory information
// ============================================================================

pub struct ProcMeminfo;

impl Inode for ProcMeminfo {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        let content = "MemTotal:       262144 kB\n\
                      MemFree:        131072 kB\n\
                      MemAvailable:   200000 kB\n\
                      Buffers:            0 kB\n\
                      Cached:             0 kB\n";
        
        let bytes = content.as_bytes();
        if offset as usize >= bytes.len() { return 0; }
        let remaining = &bytes[offset as usize..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        to_copy
    }
    
    fn write_at(&self, _o: u64, _b: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata { size: 200, mode: FileMode(0o444), file_type: FileType::File }
    }
}

// ============================================================================
// /proc/cpuinfo - CPU information
// ============================================================================

pub struct ProcCpuinfo;

impl Inode for ProcCpuinfo {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        let content = "processor\t: 0\n\
                      vendor_id\t: Aether\n\
                      cpu family\t: 6\n\
                      model name\t: Aether Virtual CPU\n\
                      cpu MHz\t\t: 1000.000\n\
                      bogomips\t: 2000.00\n";
        
        let bytes = content.as_bytes();
        if offset as usize >= bytes.len() { return 0; }
        let remaining = &bytes[offset as usize..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        to_copy
    }
    
    fn write_at(&self, _o: u64, _b: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata { size: 300, mode: FileMode(0o444), file_type: FileType::File }
    }
}

// ============================================================================
// /proc/self directory
// ============================================================================

pub struct ProcSelfDir;

impl Inode for ProcSelfDir {
    fn read_at(&self, _o: u64, _b: &mut [u8]) -> usize { 0 }
    fn write_at(&self, _o: u64, _b: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata { size: 0, mode: FileMode(0o555), file_type: FileType::Directory }
    }
    
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        Ok(alloc::vec![
            (String::from("stat"), 1),
            (String::from("maps"), 2),
        ])
    }
    
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        match name {
            "stat" => Ok(Arc::new(ProcStat::for_current())),
            "maps" => Ok(Arc::new(ProcMaps)),
            _ => Err(FsError::NotFound),
        }
    }
}

// ============================================================================
// /proc root directory
// ============================================================================

pub struct ProcFsRoot;

impl Inode for ProcFsRoot {
    fn read_at(&self, _o: u64, _b: &mut [u8]) -> usize { 0 }
    fn write_at(&self, _o: u64, _b: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata { size: 0, mode: FileMode(0o555), file_type: FileType::Directory }
    }
    
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        let mut entries = alloc::vec![
            (String::from("self"), 0),
            (String::from("meminfo"), 1),
            (String::from("cpuinfo"), 2),
        ];
        
        // Add PIDs from ALL_TASKS
        let tasks = ALL_TASKS.lock();
        for task in tasks.iter() {
            let pid = task.lock().id;
            entries.push((format!("{}", pid), pid as u64));
        }
        
        Ok(entries)
    }
    
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        match name {
            "self" => Ok(Arc::new(ProcSelfDir)),
            "meminfo" => Ok(Arc::new(ProcMeminfo)),
            "cpuinfo" => Ok(Arc::new(ProcCpuinfo)),
            _ => {
                // Try to parse as PID
                if let Ok(pid) = name.parse::<usize>() {
                    // Check if PID exists
                    let tasks = ALL_TASKS.lock();
                    if tasks.iter().any(|t| t.lock().id == pid) {
                        return Ok(Arc::new(ProcPidDir { pid }));
                    }
                }
                Err(FsError::NotFound)
            }
        }
    }
}

// ============================================================================
// /proc/[pid] directory
// ============================================================================

pub struct ProcPidDir {
    pid: usize,
}

impl Inode for ProcPidDir {
    fn read_at(&self, _o: u64, _b: &mut [u8]) -> usize { 0 }
    fn write_at(&self, _o: u64, _b: &[u8]) -> usize { 0 }
    
    fn metadata(&self) -> Metadata {
        Metadata { size: 0, mode: FileMode(0o555), file_type: FileType::Directory }
    }
    
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        Ok(alloc::vec![
            (String::from("stat"), 1),
            (String::from("maps"), 2),
        ])
    }
    
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        match name {
            "stat" => Ok(Arc::new(ProcStat::new(self.pid))),
            "maps" => Ok(Arc::new(ProcMaps)),
            _ => Err(FsError::NotFound),
        }
    }
}

// ============================================================================
// ProcFS FileSystem
// ============================================================================

pub struct ProcFs;

impl FileSystem for ProcFs {
    fn root_inode(&self) -> Arc<dyn Inode> {
        Arc::new(ProcFsRoot)
    }
}
