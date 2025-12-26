//! Run Queue

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use alloc::sync::Arc;
use crate::sched::task::Task;
use spin::Lazy;

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

/// Add a new task to the run queue
pub fn spawn_task(task: Task) -> usize {
    let pid = task.id;
    let task_arc = Arc::new(Mutex::new(task));
    
    // Add to all tasks list
    ALL_TASKS.lock().push(task_arc.clone());
    
    // Add to run queue
    RUN_QUEUE.lock().tasks.push_back(task_arc);
    
    pid
}

/// Get a task by PID
pub fn get_task_by_pid(pid: usize) -> Option<Arc<Mutex<Task>>> {
    let tasks = ALL_TASKS.lock();
    tasks.iter().find(|t| t.lock().id == pid).cloned()
}
