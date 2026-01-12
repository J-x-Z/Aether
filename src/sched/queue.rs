//! Run Queue

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use alloc::sync::Arc;
use crate::sched::task::Task;
use spin::Lazy;
use aether_core::backend::{Backend, ExitReason};

pub struct DummyBackend;
impl Backend for DummyBackend {
    fn name(&self) -> &str { "Dummy" }
    fn step(&self) -> ExitReason { ExitReason::Halt }
    unsafe fn get_framebuffer(&self, _w: usize, _h: usize) -> &[u32] { &[] }
}

pub struct RunQueue {
    pub tasks: VecDeque<Arc<Mutex<Task>>>,
}

pub static RUN_QUEUE: Lazy<Mutex<RunQueue>> = Lazy::new(|| Mutex::new(RunQueue {
    tasks: VecDeque::new(),
}));

/// Current running task (per-CPU in SMP, single for now)
pub static CURRENT_TASK: Lazy<Mutex<Option<Arc<Mutex<Task>>>>> = Lazy::new(|| Mutex::new(None));

/// All tasks in the system (for wait4/waitpid lookup)
pub static ALL_TASKS: Lazy<Mutex<Vec<Arc<Mutex<Task>>>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Add a new task to the run queue AND Scheduler
pub fn spawn_task(task: Task) -> usize {
    let pid = task.id;
    let cr3 = task.cr3;
    let stack_pointer = task.saved_rsp; // Use saved RSP
    let stack = task.stack.clone(); // Clone stack to Process (Ownership move logic preferred but clone for safety)
    
    let task_arc = Arc::new(Mutex::new(task));
    
    // Add to all tasks list (for Syscalls/FDs)
    ALL_TASKS.lock().push(task_arc.clone());
    
    // Add to legacy queue (optional)
    RUN_QUEUE.lock().tasks.push_back(task_arc);
    
    // Add to REAL Scheduler
    if let Some(mut sched_lock) = crate::globals::SCHEDULER.try_lock() {
        if let Some(sched) = sched_lock.as_mut() {
             // We must construct Process manually
             // Note: scheduler::spawn() allocates NEW stack. We want to use OUR stack.
             // So we push directly to deque.
             use aether_core::scheduler::{Process, ProcessState};
             
             sched.processes.push_back(Process {
                 id: pid as u64,
                 backend: Arc::new(DummyBackend),
                 state: ProcessState::Ready,
                 stack: stack, // Use the stack from Task
                 stack_pointer: stack_pointer as usize,
                 cr3: cr3,
             });
             log::info!("[RunQueue] Injected PID {} into Scheduler (CR3={:x})", pid, cr3);
        }
    }
    
    pid
}

/// Get a task by PID
pub fn get_task_by_pid(pid: usize) -> Option<Arc<Mutex<Task>>> {
    let tasks = ALL_TASKS.lock();
    tasks.iter().find(|t| t.lock().id == pid).cloned()
}
