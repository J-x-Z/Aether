//! Task / Process Definition

use alloc::vec::Vec;
use alloc::sync::Arc;
use crate::fs::vfs::Inode;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Process ID
pub type Pid = usize;

/// Task State
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

/// Helper struct for an open file descriptor
#[derive(Clone)]
pub struct FileDescriptor {
    pub inode: Arc<dyn Inode>,
    pub offset: u64,
    pub flags: u32,
}

/// A Process / Task Control Block
pub struct Task {
    pub id: Pid,
    pub state: TaskState,
    pub stack: Vec<u8>,
    pub stack_top: usize,
    pub fd_table: Vec<Option<FileDescriptor>>,
}

static NEXT_PID: AtomicUsize = AtomicUsize::new(1);

impl Task {
    pub fn new(stack_size: usize) -> Self {
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        let mut task = Self {
            id: pid,
            state: TaskState::Ready,
            stack: alloc::vec![0; stack_size],
            stack_top: 0, // Should be calculated based on stack ptr
            fd_table: Vec::new(),
        };
        
        // Initialize stdio
        // 0: stdin, 1: stdout, 2: stderr
        // For now, push None or a dummy console inode
        task.fd_table.push(None); // 0
        task.fd_table.push(None); // 1
        task.fd_table.push(None); // 2
        
        task
    }
    
    /// Allocate a new file descriptor
    pub fn add_file(&mut self, file: FileDescriptor) -> usize {
        // Look for free slot
        for (i, slot) in self.fd_table.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(file);
                return i;
            }
        }
        // Push new
        self.fd_table.push(Some(file));
        self.fd_table.len() - 1
    }
    
    pub fn get_file(&self, fd: usize) -> Option<&FileDescriptor> {
        if fd < self.fd_table.len() {
            self.fd_table[fd].as_ref()
        } else {
            None
        }
    }
}
