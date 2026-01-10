#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

extern crate alloc;

mod arch;
mod mm;
mod sched;
mod fs;
mod drivers;
mod syscall;

// Legacy modules - x86 only, to be refactored/removed
#[cfg(target_arch = "x86_64")]
mod backend;
#[cfg(target_arch = "x86_64")]
mod interrupts;
#[cfg(target_arch = "x86_64")]
mod video;
#[cfg(target_arch = "x86_64")]
mod multitasking;
#[cfg(target_arch = "x86_64")]
mod globals;
#[cfg(target_arch = "x86_64")]
mod keyboard;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

/// Ultra-early serial output for bare-metal debugging (before any init)
#[cfg(target_arch = "x86_64")]
#[inline(never)]
fn early_serial_print(s: &[u8]) {
    const COM1: u16 = 0x3F8;
    unsafe {
        use x86_64::instructions::port::Port;
        let mut data: Port<u8> = Port::new(COM1);
        let mut status: Port<u8> = Port::new(COM1 + 5);
        for &byte in s {
            // Wait for transmit buffer empty
            while (status.read() & 0x20) == 0 {}
            data.write(byte);
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn early_serial_print(_s: &[u8]) {}

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    // === EARLY DEBUG: Use UEFI text output to show on screen ===
    // This works before we switch to GOP graphics mode
    
    // Initialize UEFI services first
    uefi_services::init(&mut system_table).unwrap();
    system_table.stdout().reset(false).unwrap();
    
    // Helper macro for screen output
    macro_rules! screen_print {
        ($st:expr, $msg:expr) => {
            {
                use core::fmt::Write as _;
                let _ = core::fmt::Write::write_str($st.stdout(), $msg);
                let _ = core::fmt::Write::write_str($st.stdout(), "\r\n");
            }
            // Also send to serial for QEMU
            early_serial_print($msg.as_bytes());
            early_serial_print(b"\r\n");
        };
    }
    
    screen_print!(system_table, "[BOOT] Aether EFI entry point reached");
    screen_print!(system_table, "[BOOT] UEFI services initialized");
    
    log::info!("Aether Kernel 2.0 (Hybrid/POSIX) booting...");
    screen_print!(system_table, "[BOOT] Log initialized");
    
    // 1. Initialize Video (GOP) - x86 only for now
    #[cfg(target_arch = "x86_64")]
    {
        screen_print!(system_table, "[BOOT] Initializing Video (GOP)...");
        init_video(&system_table);
        // After this, UEFI text output may not work (switched to graphics mode)
        early_serial_print(b"[BOOT] Video OK\r\n");
    }
    
    // 2. Initialize Architecture
    early_serial_print(b"[BOOT] Initializing Arch...\r\n");
    log::info!("[Kernel] Initializing Architecture...");
    arch::init();
    early_serial_print(b"[BOOT] Arch OK, loading IDT...\r\n");
    
    #[cfg(target_arch = "x86_64")]
    {
        interrupts::init_idt();
        early_serial_print(b"[BOOT] IDT OK\r\n");
    }
    
    // 3. Initialize Memory Management
    early_serial_print(b"[BOOT] Initializing MM...\r\n");
    log::info!("[Kernel] Initializing Memory Management...");
    mm::init();
    
    // 4. Initialize Filesystem
    log::info!("[Kernel] Initializing Filesystem...");
    fs::init();
    
    // 5. Initialize Scheduler
    log::info!("[Kernel] Initializing Scheduler...");
    sched::init();
    
    // 6. Initialize Drivers
    log::info!("[Kernel] Initializing Drivers...");
    drivers::init();
    
    // For testing: set to true to use simple init.bin instead of BusyBox
    const USE_SIMPLE_INIT: bool = false;
    
    // 7. Load Init Process
    let init_path = if USE_SIMPLE_INIT { "/init" } else { "/bin/busybox" };
    log::info!("[Kernel] Loading {}...", init_path);
    if let Ok(inode) = fs::open(init_path, 0) {
        // Allocate buffer for binary (2MB max)
        let mut buffer = alloc::vec![0u8; 2 * 1024 * 1024];
        let len = inode.read_at(0, &mut buffer);
        log::info!("[Kernel] Read {}: {} bytes", init_path, len);
        
        if len > 64 {
            use crate::syscall::elf::{load_elf, setup_user_stack, AuxvEntry, AT_PAGESZ};
            
            // Load ELF (static binary, base = 0)
            match load_elf(&buffer[..len], 0) {
                Ok(loaded) => {
                    log::info!("[Kernel] BusyBox loaded, entry: 0x{:x}", loaded.entry_point);
                    
                    // Set up Auxv
                    let auxv = alloc::vec![
                        AuxvEntry { key: AT_PAGESZ, val: 4096 },
                    ];
                    
                    // argv: "sh" (BusyBox multi-call binary uses argv[0] to determine behavior)
                    let argv: &[&[u8]] = &[b"sh"];
                    let envp: &[&[u8]] = &[];
                    
                    // Set up stack (at high address)
                    let stack_top = 0x7FFFFF000000u64;
                    let stack_size = 128 * 1024; // 128KB
                    mm::paging::make_user_accessible(stack_top - stack_size, stack_size);
                    
                    let user_sp = setup_user_stack(stack_top, argv, envp, &auxv);
                    
                    log::info!("[Kernel] Entering BusyBox shell (Ring 3)...");
                    log::info!("[Kernel]   Entry: 0x{:x}, Stack: 0x{:x}", loaded.entry_point, user_sp);
                    
                    // CRITICAL: Allocate Kernel Stack for this process (PID 1) and update TSS!
                    // Otherwise interrupts/syscalls from Ring 3 will crash (Double Fault) due to RSP0=0.
                    let mut kernel_stack = alloc::vec![0u8; 128 * 1024]; // 128KB
                    let kernel_stack_top = (kernel_stack.as_ptr() as u64 + kernel_stack.len() as u64) & !0xF;
                    unsafe {
                        crate::arch::x86_64::gdt::set_interrupt_stack(kernel_stack_top);
                    }
                    
                    // Set up syscall kernel stack (for swapgs-based syscall handling)
                    #[cfg(target_arch = "x86_64")]
                    crate::arch::x86_64::syscall::setup_syscall_stacks(kernel_stack_top);
                    
                    // Prevent deallocation of stack
                    core::mem::forget(kernel_stack);

                    // Jump to Ring 3
                    unsafe {
                        arch::enter_usermode(loaded.entry_point, user_sp);
                    }
                }
                Err(e) => {
                    log::error!("[Kernel] Failed to load BusyBox ELF: {}", e);
                }
            }
        }
    } else {
        log::error!("[Kernel] Failed to open /bin/busybox");
    }

    log::error!("[Kernel] Init failed or returned!");
    
    // Halt Loop
    loop {
        #[cfg(target_arch = "x86_64")]
        x86_64::instructions::hlt();
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("wfi"); }
    }
}

#[cfg(target_arch = "x86_64")]
fn init_video(st: &SystemTable<Boot>) {
    let bt = st.boot_services();
    if let Ok(gop_handle) = bt.get_handle_for_protocol::<GraphicsOutput>() {
        if let Ok(mut gop) = bt.open_protocol_exclusive::<GraphicsOutput>(gop_handle) {
             let mode_info = gop.current_mode_info();
             let (width, height) = mode_info.resolution();
             let mut fb = gop.frame_buffer();
             let fb_ptr = fb.as_mut_ptr();
             let size = fb.size();
             let stride = mode_info.stride();
             
             crate::video::init(fb_ptr, size, width, height, stride);
             log::info!("[Video] Initialized {}x{} (stride: {})", width, height, stride);
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn test_syscalls() {
    log::info!("[Test] Testing POSIX syscalls internally...");
    
    // Test open (should fail as file doesn't exist yet, or succeed if we stubbed it)
    let ret = syscall::dispatch(syscall::numbers::SYS_OPEN, 0, 0, 0); // filename=NULL
    log::info!("[Test] open(NULL) = {}", ret);
    
    // Test write to stdout (fd=1)
    let msg = "Hello from Internal Syscall!\n";
    let ptr = msg.as_ptr() as usize;
    let len = msg.len();
    let ret = syscall::dispatch(syscall::numbers::SYS_WRITE, 1, ptr, len);
    log::info!("[Test] write(1, ...) = {}", ret);
}
