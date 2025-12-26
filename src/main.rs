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

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    system_table.stdout().reset(false).unwrap();
    
    log::info!("Aether Kernel 2.0 (Hybrid/POSIX) booting...");
    
    // 1. Initialize Video (GOP) - x86 only for now
    #[cfg(target_arch = "x86_64")]
    init_video(&system_table);
    
    // 2. Initialize Architecture
    log::info!("[Kernel] Initializing Architecture...");
    arch::init();
    #[cfg(target_arch = "x86_64")]
    interrupts::init_idt(); // Use legacy interrupt handler for now as arch::idt is stub
    
    // 3. Initialize Memory Management
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
    
    // 7. Load Init Process
    log::info!("[Kernel] Loading /init...");
    if let Ok(inode) = fs::open("/init", 0) {
        // Allocate buffer for init (64KB max for now)
        let mut buffer = alloc::vec![0u8; 65536];
        let len = inode.read_at(0, &mut buffer);
        log::info!("[Kernel] Read init: {} bytes", len);
        
        if len > 0 {
            // Map Userspace Memory (Code: 0x400000)
            let code_addr = 0x400000;
            mm::paging::make_user_accessible(code_addr, len as u64);
            
            // Map Userspace Stack (Stack: 0x600000)
            let stack_addr = 0x600000;
            let stack_size = 4096 * 4;
            mm::paging::make_user_accessible(stack_addr, stack_size as u64);
            
            // Copy Code to Userspace Address
            unsafe {
                core::ptr::copy_nonoverlapping(buffer.as_ptr(), code_addr as *mut u8, len);
            }
            
            log::info!("[Kernel] Entering Userspace (Ring 3)...");
            
            // Jump to Ring 3
            // Stack grows down, so pointer is top of stack region
            unsafe {
                arch::enter_usermode(code_addr, stack_addr + stack_size as u64);
            }
        }
    } else {
        log::error!("[Kernel] Failed to open /init");
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
