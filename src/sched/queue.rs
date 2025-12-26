//! Run Queue

use alloc::collections::VecDeque;
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

/// Dummy Current Task Holder (for single-core MVP without full scheduler)
/// In a real system, this would be per-CPU data
pub static CURRENT_TASK: Lazy<Mutex<Option<Arc<Mutex<Task>>>>> = Lazy::new(|| Mutex::new(None));
