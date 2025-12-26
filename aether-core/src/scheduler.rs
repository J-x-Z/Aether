use crate::backend::Backend;
use alloc::sync::Arc;
use alloc::collections::VecDeque;

pub type ProcessId = u64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

pub struct Process {
    pub id: ProcessId,
    pub backend: Arc<dyn Backend>,
    pub state: ProcessState,
    pub stack: alloc::vec::Vec<u8>,
    pub stack_pointer: usize,
}

pub struct Scheduler {
    pub processes: VecDeque<Process>,
    pub next_pid: ProcessId,
    pub current_pid: Option<ProcessId>,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            processes: VecDeque::new(),
            next_pid: 1,
            current_pid: None,
        }
    }

    pub fn spawn(&mut self, backend: Arc<dyn Backend>) -> ProcessId {
        let pid = self.next_pid;
        self.next_pid += 1;
        
        // Allocate 128KB Kernel Stack
        let stack_size = 128 * 1024;
        let mut stack = alloc::vec![0u8; stack_size];
        
        // Stack grows down. Initial SP is end of stack.
        // We essentially "leak" the Vec's buffer address to SP.
        let stack_start = stack.as_ptr() as usize;
        let stack_end = stack_start + stack_size;
        
        // Ensure 16-byte alignment
        let stack_pointer = stack_end & !0xF;

        self.processes.push_back(Process {
            id: pid,
            backend,
            state: ProcessState::Ready,
            stack,
            stack_pointer,
        });
        
        log::info!("[Scheduler] Spawned Process {} (SP: {:x})", pid, stack_pointer);
        pid
    }

    /// Simple Round-Robin Scheduler
    /// Returns the PID of the process to switch TO.
    /// Returns None if no process is ready (or only 1 process running).
    pub fn schedule(&mut self) -> Option<ProcessId> {
        if self.processes.is_empty() {
            return None;
        }

        let current_index = self.current_pid.and_then(|pid| {
            self.processes.iter().position(|p| p.id == pid)
        }).unwrap_or(0);

        // Find next Ready process starting from current + 1
        let mut next_index = current_index;
        
        // Try to find a Ready process (scan at most once full circle)
        for _ in 0..self.processes.len() {
            next_index = (next_index + 1) % self.processes.len();
            if self.processes[next_index].state == ProcessState::Ready || 
               self.processes[next_index].state == ProcessState::Running {
                
                let next_pid = self.processes[next_index].id;
                
                // If it's the same as current, no switch needed?
                // Actually for preemptive time slicing, we might want to switch to self 
                // to refresh time slice, but here we just return None to avoid overhead.
                if let Some(curr) = self.current_pid {
                    if curr == next_pid {
                        return None;
                    }
                }
                
                self.current_pid = Some(next_pid);
                return Some(next_pid);
            }
        }
        
        // No ready process found
        None
    }

    /// Get process by ID (mutable)
    pub fn get_process_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.id == pid)
    }
}
