//! Memory Management Subsystem

pub mod pmm;     // Physical Memory Manager
pub mod vmm;     // Virtual Memory Manager
pub mod heap;    // Kernel Heap Allocator

/// Initialize memory management
pub fn init() {
    // TODO: Setup page tables, heap
}
