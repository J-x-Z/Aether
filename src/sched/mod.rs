//! Process Scheduler

pub mod task;    // Task/Process struct
pub mod queue;   // Run queue

use alloc::sync::Arc;
use spin::Mutex;
use task::Task;
use queue::{CURRENT_TASK, RUN_QUEUE};

/// Initialize scheduler
pub fn init() {
    log::info!("[Sched] Initializing Scheduler...");
    
    // Create PID 1 (Init Task)
    // For now, it's just a kernel thread context
    let init_task = Arc::new(Mutex::new(Task::new(16384)));
    
    // Set as current
    *CURRENT_TASK.lock() = Some(init_task.clone());
    
    // Add to run queue
    RUN_QUEUE.lock().tasks.push_back(init_task);
    
    log::info!("[Sched] Initialized PID 1");
}

/// Schedule next task (called from timer interrupt)
pub fn schedule() {
    // TODO: CFS-like scheduling
    // Simple round robin stub
}
