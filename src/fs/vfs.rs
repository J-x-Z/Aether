//! Virtual Filesystem Framework

use alloc::vec::Vec;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt;

/// File types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Device,
    Pipe,
    Symlink,
}

/// File permission/mode flags
#[derive(Debug, Clone, Copy)]
pub struct FileMode(pub u32);

impl FileMode {
    pub const READ: u32 = 0o4;
    pub const WRITE: u32 = 0o2;
    pub const EXEC: u32 = 0o1;
}

/// Metadata for a file/inode
pub struct Metadata {
    pub size: u64,
    pub mode: FileMode,
    pub file_type: FileType,
}

/// Inode trait - represents an object in the filesystem (file or dir)
pub trait Inode: Send + Sync {
    /// Read data from file at offset
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize;
    
    /// Write data to file at offset
    fn write_at(&self, offset: u64, buf: &[u8]) -> usize;
    
    /// Get file metadata
    fn metadata(&self) -> Metadata;
    
    /// List directory contents (returns (name, inode_ptr) tuples)
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        Err(FsError::NotADirectory)
    }

    /// Lookup entry in directory
    fn lookup(&self, _name: &str) -> Result<Arc<dyn Inode>, FsError> {
        Err(FsError::NotADirectory)
    }
}

/// FileSystem trait
pub trait FileSystem: Send + Sync {
    /// Get the root inode
    fn root_inode(&self) -> Arc<dyn Inode>;
}

/// VFS Errors
#[derive(Debug)]
pub enum FsError {
    NotFound,
    PermissionDenied,
    NotADirectory,
    IsADirectory,
    IOError,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
