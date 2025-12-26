//! Architecture-specific code for x86_64

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod syscall;

/// Initialize x86_64 architecture
pub fn init() {
    // TODO: Setup GDT, TSS, IDT
    syscall::init();
}
