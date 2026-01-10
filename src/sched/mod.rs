use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;
use crate::globals::SCHEDULER;
use crate::interrupts::TICKS;
use core::sync::atomic::Ordering;

// Restore legacy modules for syscall compatibility
pub mod task;
pub mod queue;

lazy_static! {
    /// List of sleeping tasks: (PID, WakeupTick)
    pub static ref SLEEPING: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
}

pub fn init() {
    log::info!("[Sched] Initialized (Using aether-core scheduler)");
}

/// Called by Timer Interrupt
pub fn check_timers() {
    let current_tick = TICKS.load(Ordering::Relaxed);
    let mut tasks = SLEEPING.lock();
    let mut wake_pids = Vec::new();
    
    // 1. Identify tasks to wake
    tasks.retain(|(pid, wakeup)| {
        if current_tick >= *wakeup {
            wake_pids.push(*pid);
            false // Remove from list
        } else {
            true // Keep waiting
        }
    });

    // 2. Wake them up
    if !wake_pids.is_empty() {
        if let Some(mut sched_lock) = SCHEDULER.try_lock() {
            if let Some(sched) = sched_lock.as_mut() {
                for pid in wake_pids {
                    if let Some(proc) = sched.get_process_mut(pid) {
                        proc.state = aether_core::scheduler::ProcessState::Ready;
                    }
                }
            }
        }
    }
}

pub fn sleep(duration_ms: u64) {
    // 10ms per tick (100Hz)
    let ticks = duration_ms / 10; 
    let current = TICKS.load(Ordering::Relaxed);
    let target = current + ticks;
    
    // Get Current PID
    let pid = {
        let lock = SCHEDULER.lock();
        if let Some(sched) = lock.as_ref() {
            sched.current_pid
        } else {
            None
        }
    };

    if let Some(pid) = pid {
        // 1. Add to Sleeping List
        SLEEPING.lock().push((pid, target));
        
        // 2. Mark as Blocked
        {
            let mut lock = SCHEDULER.lock();
            if let Some(sched) = lock.as_mut() {
                if let Some(proc) = sched.get_process_mut(pid) {
                    proc.state = aether_core::scheduler::ProcessState::Blocked;
                }
            }
        }
        
        // 3. Yield (Wait for next interrupt to switch us out)
        // Since we are Blocked, the scheduler will pick someone else.
        // We ensure we wait until then.
        unsafe { core::arch::asm!("hlt"); }
    }
}

pub fn yield_now() {
    unsafe { core::arch::asm!("hlt"); }
}

