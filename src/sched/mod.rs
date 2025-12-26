//! Process Scheduler

pub mod task;    // Task/Process struct
pub mod queue;   // Run queue

/// Initialize scheduler
pub fn init() {
    // TODO: Setup idle task, run queue
}

/// Schedule next task (called from timer interrupt)
pub fn schedule() {
    // TODO: CFS-like scheduling
}
