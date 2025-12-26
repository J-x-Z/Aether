//! Virtual Filesystem Layer

pub mod vfs;     // VFS abstraction
pub mod ramfs;   // In-memory filesystem
pub mod initrd;  // Initial RAM Disk loading (stub)

use alloc::sync::Arc;
use vfs::{FileSystem, Inode};
use spin::RwLock;

/// Global VFS Root
pub static ROOT: RwLock<Option<Arc<dyn Inode>>> = RwLock::new(None);

/// Initialize filesystem layer
pub fn init() {
    log::info!("[VFS] Initializing Virtual Filesystem...");
    let ramfs = ramfs::RamFS::new();
    
    // Load initrd
    let init_data = initrd::load();
    ramfs.add_file("init", init_data);
    log::info!("[VFS] Added /init to RamFS");

    let root = ramfs.root_inode();
    
    // Mount root
    *ROOT.write() = Some(root);
    log::info!("[VFS] Mounted ROOT (RamFS)");
}

/// Open a file by path
pub fn open(path: &str, _flags: u32) -> Result<Arc<dyn Inode>, vfs::FsError> {
    // TODO: Proper path resolution
    // For now, only support root-level file lookup
    let root_guard = ROOT.read();
    let root = root_guard.as_ref().ok_or(vfs::FsError::NotFound)?;
    
    if path == "/" {
        return Ok(root.clone());
    }
    
    // Simple lookup for "/filename"
    let filename = if path.starts_with('/') {
        &path[1..]
    } else {
        path
    };
    
    // Lookup in root directory
    root.lookup(filename)
}
