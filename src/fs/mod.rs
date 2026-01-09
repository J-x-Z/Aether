//! Virtual Filesystem Layer

pub mod vfs;     // VFS abstraction
pub mod ramfs;   // In-memory filesystem
pub mod initrd;  // Initial RAM Disk loading (stub)
pub mod devfs;   // /dev virtual filesystem
pub mod procfs;  // /proc virtual filesystem

use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use vfs::{FileSystem, Inode};
use spin::RwLock;

/// Mount point entry
struct MountPoint {
    path: String,
    fs: Arc<dyn FileSystem>,
}

/// Global mount table
static MOUNTS: RwLock<Vec<MountPoint>> = RwLock::new(Vec::new());

/// Global VFS Root
pub static ROOT: RwLock<Option<Arc<dyn Inode>>> = RwLock::new(None);

/// Initialize filesystem layer
pub fn init() {
    log::info!("[VFS] Initializing Virtual Filesystem...");
    
    // Create root RamFS
    let ramfs = ramfs::RamFS::new();
    let init_data = initrd::load();
    ramfs.add_file("init", init_data);
    log::info!("[VFS] Added /init to RamFS");
    
    let root = ramfs.root_inode();
    *ROOT.write() = Some(root);
    log::info!("[VFS] Mounted ROOT (RamFS)");
    
    // Mount /dev
    let devfs = Arc::new(devfs::DevFs::new());
    mount("/dev", devfs);
    log::info!("[VFS] Mounted /dev (DevFS)");
    
    // Mount /proc
    let procfs = Arc::new(procfs::ProcFs);
    mount("/proc", procfs);
    log::info!("[VFS] Mounted /proc (ProcFS)");
}

/// Mount a filesystem at path
pub fn mount(path: &str, fs: Arc<dyn FileSystem>) {
    let mut mounts = MOUNTS.write();
    mounts.push(MountPoint {
        path: String::from(path),
        fs,
    });
}

/// Open a file by path
pub fn open(path: &str, _flags: u32) -> Result<Arc<dyn Inode>, vfs::FsError> {
    // Check mount points first (longest match wins)
    {
        let mounts = MOUNTS.read();
        let mut best_match: Option<(&MountPoint, &str)> = None;
        
        for mp in mounts.iter() {
            if path.starts_with(&mp.path) {
                let suffix = &path[mp.path.len()..];
                // Ensure it's a proper path match (not just prefix)
                if suffix.is_empty() || suffix.starts_with('/') {
                    if best_match.is_none() || mp.path.len() > best_match.unwrap().0.path.len() {
                        let sub_path = if suffix.starts_with('/') { &suffix[1..] } else { suffix };
                        best_match = Some((mp, sub_path));
                    }
                }
            }
        }
        
        if let Some((mp, sub_path)) = best_match {
            let root = mp.fs.root_inode();
            if sub_path.is_empty() {
                return Ok(root);
            }
            return resolve_path(root, sub_path);
        }
    }
    
    // Fall back to root FS
    let root_guard = ROOT.read();
    let root = root_guard.as_ref().ok_or(vfs::FsError::NotFound)?;
    
    if path == "/" {
        return Ok(root.clone());
    }
    
    let filename = if path.starts_with('/') { &path[1..] } else { path };
    resolve_path(root.clone(), filename)
}

/// Resolve a multi-component path from a starting inode
fn resolve_path(start: Arc<dyn Inode>, path: &str) -> Result<Arc<dyn Inode>, vfs::FsError> {
    let mut current = start;
    
    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        current = current.lookup(component)?;
    }
    
    Ok(current)
}
