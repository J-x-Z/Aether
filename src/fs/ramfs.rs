//! Simple RAM Filesystem

use alloc::sync::Arc;
use alloc::string::String;
use alloc::collections::BTreeMap;
use spin::RwLock;
use alloc::vec::Vec;
use crate::fs::vfs::{self, FileSystem, Inode, Metadata, FileType, FileMode, FsError};

/// RamFS structure
pub struct RamFS {
    root: Arc<RamNode>,
}

impl RamFS {
    pub fn new() -> Self {
        Self {
            root: Arc::new(RamNode::new_dir()),
        }
    }
    
    pub fn add_file(&self, name: &str, content: Vec<u8>) {
         let mut guard = self.root.data.write();
         if let RamNodeData::Directory { children } = &mut *guard {
             children.insert(String::from(name), Arc::new(RamNode::new_file(content)));
         }
    }
}

impl FileSystem for RamFS {
    fn root_inode(&self) -> Arc<dyn Inode> {
        self.root.clone()
    }
}

/// Node in RamFS (File or Directory)
struct RamNode {
    data: RwLock<RamNodeData>,
}

enum RamNodeData {
    File {
        content: Vec<u8>,
    },
    Directory {
        children: BTreeMap<String, Arc<RamNode>>,
    },
}

impl RamNode {
    fn new_dir() -> Self {
        Self {
            data: RwLock::new(RamNodeData::Directory {
                children: BTreeMap::new(),
            }),
        }
    }
    
    fn new_file(content: Vec<u8>) -> Self {
        Self {
            data: RwLock::new(RamNodeData::File { content }),
        }
    }
}

impl Inode for RamNode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        let guard = self.data.read();
        match &*guard {
            RamNodeData::File { content } => {
                let off = offset as usize;
                if off >= content.len() {
                    return 0;
                }
                let len = core::cmp::min(buf.len(), content.len() - off);
                buf[..len].copy_from_slice(&content[off..off + len]);
                len
            }
            RamNodeData::Directory { .. } => 0, // Cannot read dir as file
        }
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> usize {
        let mut guard = self.data.write();
        match &mut *guard {
            RamNodeData::File { content } => {
                let off = offset as usize;
                let end = off + buf.len();
                if end > content.len() {
                    content.resize(end, 0);
                }
                content[off..end].copy_from_slice(buf);
                buf.len()
            }
            RamNodeData::Directory { .. } => 0, // Cannot write to dir directly
        }
    }

    fn metadata(&self) -> Metadata {
        let guard = self.data.read();
        match &*guard {
            RamNodeData::File { content } => Metadata {
                size: content.len() as u64,
                mode: FileMode(FileMode::READ | FileMode::WRITE),
                file_type: FileType::File,
            },
            RamNodeData::Directory { .. } => Metadata {
                size: 0,
                mode: FileMode(FileMode::READ | FileMode::WRITE | FileMode::EXEC),
                file_type: FileType::Directory,
            },
        }
    }
    
    fn poll(&self) -> Result<Vec<(String, u64)>, FsError> {
        let guard = self.data.read();
        match &*guard {
            RamNodeData::Directory { children } => {
                let mut entries = Vec::new();
                for (name, _) in children.iter() {
                    // TODO: Return actual inode number if we tracked it
                    entries.push((name.clone(), 0)); 
                }
                Ok(entries)
            }
            _ => Err(FsError::NotADirectory),
        }
    }
    
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        let guard = self.data.read();
        match &*guard {
            RamNodeData::Directory { children } => {
                if let Some(node) = children.get(name) {
                     Ok(node.clone())
                } else {
                     Err(FsError::NotFound)
                }
            }
            _ => Err(FsError::NotADirectory),
        }
    }
}
