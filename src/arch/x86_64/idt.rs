//! Interrupt Descriptor Table (IDT) for x86_64
//!
//! Note: The main IDT is in kernel-uefi/src/interrupts.rs
//! This module provides architecture-specific IDT helpers.

/// Re-export from main interrupts module
pub use crate::interrupts::init_idt;
