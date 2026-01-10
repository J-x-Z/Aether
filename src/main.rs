#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]
#![feature(alloc_error_handler)]

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
    // Serial port can hang bare metal if not present (reading 0x00 from status = infinite loop)
    // Default to FALSE for safety on real hardware. Enable for QEMU only.
    const ENABLE_SERIAL: bool = false;
    
    if !ENABLE_SERIAL { return; }

    const COM1: u16 = 0x3F8;
    unsafe {
        use x86_64::instructions::port::Port;
        let mut data: Port<u8> = Port::new(COM1);
        let mut status: Port<u8> = Port::new(COM1 + 5);
        for &byte in s {
            // Simple wait with timeout or just write (safer for bare metal)
            // If status reads 0, we might hang forever.
            // Just write and hope for the best to avoid hanging.
            data.write(byte);
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn early_serial_print(_s: &[u8]) {}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    early_serial_print(b"PANIC: ");
    if let Some(location) = info.location() {
        use core::fmt::Write;
        // We can't easily format to serial without a writer, but we can try simple output
        // Or just hang.
    }
    early_serial_print(b"KERNEL PANIC\r\n");
    loop {
        core::hint::spin_loop(); 
    }
}

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    // === EARLY DEBUG: Use UEFI text output to show on screen ===
    // This works before we switch to GOP graphics mode
    
    // Manual Logger Init (Simple Serial Log for now, or just rely on screen_print)
    // uefi_services::init(&mut system_table).unwrap(); <-- REMOVED
    
    // Initialize UEFI services/console manually if needed
    // But SystemTable is already usable.
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
    
    // Initialize UEFI Input (for Hyper-V keyboard)
    const ENABLE_UEFI_INPUT: bool = false;
    if ENABLE_UEFI_INPUT {
        unsafe {
            drivers::uefi_input::init_protocol(system_table.stdin() as *mut _);
        }
        screen_print!(system_table, "[BOOT] UEFI Input driver registered");
    } else {
        screen_print!(system_table, "[BOOT] UEFI Input disabled (debugging crash)");
    }

    // 1. Initialize Video (GOP) - x86 only for now
    // Re-enabled with detailed step-by-step debugging
    const ENABLE_GOP: bool = true;
    #[cfg(target_arch = "x86_64")]
    if ENABLE_GOP {
        screen_print!(system_table, "[BOOT] About to init GOP...");
        screen_print!(system_table, "[BOOT] GOP init returned");
        early_serial_print(b"[BOOT] Video OK\r\n");
        // Removed GOP stall as it passed
    } else {
        screen_print!(system_table, "[BOOT] GOP disabled for debugging");
    }
    
    // 2. Initialize Architecture
    early_serial_print(b"[BOOT] Initializing Arch...\r\n");
    log::info!("[Kernel] Initializing Architecture...");
    
    // DEBUG: Dump current segment selectors
    unsafe {
        use x86_64::instructions::segmentation::{CS, DS, ES, SS, FS, GS, Segment};
        let cs = CS::get_reg().0;
        let ds = DS::get_reg().0;
        let ss = SS::get_reg().0;
        let es = ES::get_reg().0;
        let fs = FS::get_reg().0;
        let gs = GS::get_reg().0;
        
        use core::fmt::Write;
        let _ = write!(system_table.stdout(), "[Arch] Current Segments: CS={:x} SS={:x} DS={:x} ES={:x} FS={:x} GS={:x}\r\n", cs, ss, ds, es, fs, gs);
    }
    
    // Manual arch::init expansion for granular debugging
    unsafe { core::arch::asm!("cli", options(nomem, nostack)); }
    screen_print!(system_table, "[Arch] Interrupts disabled (CLI)");
    // Removed stall here as it might hang on some firmware with CLI

    screen_print!(system_table, "[Arch] Loading GDT...");
    arch::gdt::init();
    screen_print!(system_table, "[Arch] GDT loaded - RELOADING SEGMENTS DONE");

    screen_print!(system_table, "[Arch] Initializing Syscall MSRs...");
    arch::syscall::init();
    screen_print!(system_table, "[Arch] Syscall MSRs set");
    
    early_serial_print(b"[BOOT] Arch OK, loading IDT...\r\n");

    #[cfg(target_arch = "x86_64")]
    {
        interrupts::init_idt();
        log::info!("[Kernel] IDT initialized");
    }
    
    // 3. Initialize Memory Management
    
    // 3. Initialize Memory Management
    early_serial_print(b"[BOOT] Initializing MM...\r\n");
    screen_print!(system_table, "[Kernel] Initializing Memory Management...");
    // STALL 0.5s
    system_table.boot_services().stall(500_000);

    let (phys_offset, memory_map_iter) = mm::init(&system_table);
    screen_print!(system_table, "[Kernel] Memory Management initialized");
    
    // 4. Initialize Filesystem (RamFS)
    early_serial_print(b"[BOOT] Initializing FS...\r\n");
    screen_print!(system_table, "[Kernel] Initializing Filesystem...");
    fs::init(phys_offset);
    // STALL 0.5s
    system_table.boot_services().stall(500_000);
    
    // 5. Initialize Scheduler
    screen_print!(system_table, "[Kernel] Initializing Scheduler...");
    sched::init();
    
    // 6. Initialize Drivers
    screen_print!(system_table, "[Kernel] Initializing Drivers...");
    drivers::init();
    system_table.boot_services().stall(500_000); // STALL 0.5s

    // 7. Enable Interrupts (Testing in QEMU)
    screen_print!(system_table, "[Kernel] About to enable interrupts (STI)...");
    system_table.boot_services().stall(1_000_000); // STALL 1s
    #[cfg(target_arch = "x86_64")]
    {
        interrupts::enable();
        screen_print!(system_table, "[Kernel] Interrupts ENABLED");
    }
    system_table.boot_services().stall(1_000_000); // STALL 1s
    
    // For testing: set to true to use simple init.bin instead of BusyBox
    const USE_SIMPLE_INIT: bool = false;
    
    // 7. Load Init Process
    let init_path = if USE_SIMPLE_INIT { "/init" } else { "/bin/busybox" };
    screen_print!(system_table, "[Kernel] Loading init...");

    screen_print!(system_table, "[Kernel] DEBUG: About to call fs::open");

    if let Ok(inode) = fs::open(init_path, 0) {
        screen_print!(system_table, "[Kernel] DEBUG: fs::open succeeded");
        
        // Allocate buffer for binary (2MB max)
        screen_print!(system_table, "[Kernel] DEBUG: Allocating 2MB buffer...");
        let mut buffer = alloc::vec![0u8; 2 * 1024 * 1024];
        screen_print!(system_table, "[Kernel] DEBUG: Buffer allocated, reading...");
        let len = inode.read_at(0, &mut buffer);
        screen_print!(system_table, "[Kernel] DEBUG: Read bytes");
        
        system_table.boot_services().stall(1_000_000); // STALL 1s

        if len > 64 {
            use crate::syscall::elf::{load_elf, setup_user_stack, AuxvEntry, AT_PAGESZ};
            
            screen_print!(system_table, "[Kernel] DEBUG: About to load ELF...");
            system_table.boot_services().stall(3_000_000); // STALL 3s

            // Load ELF (static binary, base = 0)
            match load_elf(&buffer[..len], 0) {
                Ok(loaded) => {
                    screen_print!(system_table, "[Kernel] BusyBox loaded!");
                    system_table.boot_services().stall(1_000_000); // STALL 1s
                    
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
                    
                    screen_print!(system_table, "[Kernel] Mapping user stack...");
                    mm::paging::make_user_accessible(stack_top - stack_size, stack_size);
                    screen_print!(system_table, "[Kernel] User stack mapped!");
                    
                    screen_print!(system_table, "[Kernel] Setting up user stack...");
                    let user_sp = setup_user_stack(stack_top, argv, envp, &auxv);
                    screen_print!(system_table, "[Kernel] User stack set up!");
                    
                    screen_print!(system_table, "[Kernel] Setting up kernel stack for TSS...");
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
                    screen_print!(system_table, "[Kernel] Kernel stack ready!");

                    screen_print!(system_table, "[Kernel] STALL 3s before User Mode...");
                    system_table.boot_services().stall(3_000_000); // STALL 3s

                    screen_print!(system_table, "[Kernel] Jumping to Ring 3...");
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
    system_table.boot_services().stall(30_000_000); // STALL for error reading
    
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
    // Use log::info! for screen visibility (goes to UEFI ConOut)
    log::info!("[GOP] Step 1: Getting boot services...");
    let bt = st.boot_services();
    
    log::info!("[GOP] Step 2: Getting GOP handle...");
    let gop_handle = match bt.get_handle_for_protocol::<GraphicsOutput>() {
        Ok(h) => {
            log::info!("[GOP] Step 2: OK - got handle");
            h
        },
        Err(e) => {
            log::warn!("[GOP] Step 2: FAILED - no GOP: {:?}", e);
            return;
        }
    };
    
    log::info!("[GOP] Step 3: Opening GOP protocol (non-exclusive)...");
    // Use unsafe open_protocol with GET_PROTOCOL attribute instead of exclusive
    // Hyper-V may already have GOP open, exclusive access fails
    use uefi::table::boot::OpenProtocolAttributes;
    use uefi::table::boot::OpenProtocolParams;
    let params = OpenProtocolParams {
        handle: gop_handle,
        agent: st.boot_services().image_handle(),
        controller: None,
    };
    let gop_result = unsafe {
        bt.open_protocol::<GraphicsOutput>(params, OpenProtocolAttributes::GetProtocol)
    };
    let mut gop = match gop_result {
        Ok(g) => {
            log::info!("[GOP] Step 3: OK - protocol opened");
            g
        },
        Err(e) => {
            log::warn!("[GOP] Step 3: FAILED - cannot open: {:?}", e);
            return;
        }
    };
    
    log::info!("[GOP] Step 4: Getting mode info...");
    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    let stride = mode_info.stride();
    log::info!("[GOP] Step 4: OK - {}x{} stride={}", width, height, stride);
    
    log::info!("[GOP] Step 5: Getting framebuffer...");
    let mut fb = gop.frame_buffer();
    log::info!("[GOP] Step 5: OK - got framebuffer");
    
    log::info!("[GOP] Step 6: Getting framebuffer pointer...");
    let fb_ptr = fb.as_mut_ptr();
    let size = fb.size();
    log::info!("[GOP] Step 6: OK - ptr={:p} size={}", fb_ptr, size);
    
    log::info!("[GOP] Step 7: Initializing video subsystem...");
    crate::video::init(fb_ptr, size, width, height, stride);
    log::info!("[GOP] COMPLETE - video initialized successfully");
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
