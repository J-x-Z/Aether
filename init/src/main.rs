#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::global_asm;

// Force entry point to be raw assembly
global_asm!(r#"
    .section .text.entry
    .global _start
_start:
    // Infinite loop: EB FE
    .byte 0xeb, 0xfe
"#);

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
// Minimal Init

