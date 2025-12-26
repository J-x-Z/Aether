//! Global Descriptor Table (GDT) for x86_64
//!
//! The GDT defines memory segments and privilege levels.
//! In long mode, segmentation is mostly disabled, but we still need:
//! - Null descriptor
//! - Kernel code segment (CS)
//! - Kernel data segment (DS/SS)
//! - User code segment
//! - User data segment
//! - TSS descriptor

/// Initialize GDT
pub fn init() {
    // TODO: Setup GDT with proper segments
}
