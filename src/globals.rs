use spin::Mutex;
use aether_core::scheduler::Scheduler;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
}

// Storage for the Idle/Boot thread's stack pointer
pub static mut IDLE_STACK_POINTER: usize = 0;
