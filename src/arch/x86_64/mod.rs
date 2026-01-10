//! Architecture-specific code for x86_64

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod syscall;

/// Initialize x86_64 architecture
pub fn init() {
    // CRITICAL: Disable ALL interrupts before touching GDT.
    // UEFI has its own IDT and timer running. If an interrupt fires
    // after we load our GDT but before our IDT is ready, the CPU will
    // use UEFI's IDT entries which reference UEFI's old GDT selectors
    // (like CS=0x38) that no longer exist in our GDT -> GPF -> Triple Fault.
    unsafe { core::arch::asm!("cli", options(nomem, nostack)); }
    
    gdt::init();
    // interrupts::init_idt(); // Moved to main.rs for now or here
    syscall::init();
}

/// Jump to userspace (Ring 3)
/// Does not return.
pub unsafe fn enter_usermode(entry_point: u64, stack_pointer: u64) -> ! {
    let user_cs = gdt::user_cs();
    let user_ds = gdt::user_ds();
    
    // DEBUG: Print selector values before use (direct serial for QEMU visibility)
    crate::drivers::console::write_serial(b'C');
    crate::drivers::console::write_serial(b'S');
    crate::drivers::console::write_serial(b':');
    // Print user_cs as hex
    let cs_high = (user_cs >> 4) & 0xF;
    let cs_low = user_cs & 0xF;
    crate::drivers::console::write_serial(if cs_high < 10 { b'0' + cs_high as u8 } else { b'A' + (cs_high - 10) as u8 });
    crate::drivers::console::write_serial(if cs_low < 10 { b'0' + cs_low as u8 } else { b'A' + (cs_low - 10) as u8 });
    crate::drivers::console::write_serial(b' ');
    crate::drivers::console::write_serial(b'S');
    crate::drivers::console::write_serial(b'S');
    crate::drivers::console::write_serial(b':');
    let ds_high = (user_ds >> 4) & 0xF;
    let ds_low = user_ds & 0xF;
    crate::drivers::console::write_serial(if ds_high < 10 { b'0' + ds_high as u8 } else { b'A' + (ds_high - 10) as u8 });
    crate::drivers::console::write_serial(if ds_low < 10 { b'0' + ds_low as u8 } else { b'A' + (ds_low - 10) as u8 });
    crate::drivers::console::write_serial(b'\n');
    
    // RFLAGS: Interrupts enabled (bit 9), Reserved (bit 1) should be 1
    let rflags: u64 = 0x202; 
    
    // IRETQ Stack Frame: SS, RSP, RFLAGS, CS, RIP
    core::arch::asm!(
        "cli", // Disable interrupts while setting up segments
        
        // Load NULL into data segment registers (safe in Ring0)
        "xor ax, ax",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        
        "push {ss}",  // SS (user data selector)
        "push {rsp}", // RSP
        "push {rflags}", // RFLAGS
        "push {cs}",  // CS (user code selector)
        "push {rip}", // RIP
        
        "iretq",
        ss = in(reg) user_ds as u64, // Pushed as u64
        rsp = in(reg) stack_pointer,
        rflags = in(reg) rflags,
        cs = in(reg) user_cs as u64,
        rip = in(reg) entry_point,
        options(noreturn)
    );
}
