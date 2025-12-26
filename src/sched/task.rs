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
    pub parent_id: Pid,
    pub state: TaskState,
    pub stack: Vec<u8>,
    pub stack_top: usize,
    pub fd_table: Vec<Option<FileDescriptor>>,
    // Saved context for context switching
    pub saved_rsp: u64,
    pub saved_rip: u64,
    // Exit status
    pub exit_status: i32,
}

static NEXT_PID: AtomicUsize = AtomicUsize::new(1);

impl Task {
    pub fn new(stack_size: usize) -> Self {
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        let mut task = Self {
            id: pid,
            parent_id: 0, // Init has no parent
            state: TaskState::Ready,
            stack: alloc::vec![0; stack_size],
            stack_top: 0,
            fd_table: Vec::new(),
            saved_rsp: 0,
            saved_rip: 0,
            exit_status: 0,
        };
        
        // Initialize stdio
        task.fd_table.push(None); // 0: stdin
        task.fd_table.push(None); // 1: stdout
        task.fd_table.push(None); // 2: stderr
        
        task
    }
    
    /// Fork this task - create a copy with new PID
    pub fn fork(&self, child_rsp: u64, child_rip: u64) -> Self {
        let child_pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        
        Self {
            id: child_pid,
            parent_id: self.id,
            state: TaskState::Ready,
            stack: self.stack.clone(),
            stack_top: self.stack_top,
            fd_table: self.fd_table.clone(),
            saved_rsp: child_rsp,
            saved_rip: child_rip,
            exit_status: 0,
        }
    }
    
    /// Allocate a new file descriptor
    pub fn add_file(&mut self, file: FileDescriptor) -> usize {
        for (i, slot) in self.fd_table.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(file);
                return i;
            }
        }
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
