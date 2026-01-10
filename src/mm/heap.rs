// Kernel Heap Allocator
// Uses linked_list_allocator to provide "Vec/Box" support

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the kernel heap
/// Must be called once during kernel initialization with a valid memory region
pub unsafe fn init(start: usize, size: usize) {
    ALLOCATOR.lock().init(start as *mut u8, size);
    log::info!("[Heap] Initialized at 0x{:x}, size: {} MB", start, size / 1024 / 1024);
}

// Handler for alloc error
#[alloc_error_handler]
fn alloc_error_handler(layout: core::alloc::Layout) -> ! {
    panic!("[Heap] OOM: Failed to allocate {} bytes (align {})", layout.size(), layout.align());
}
