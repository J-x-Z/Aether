#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;
mod backend;
mod interrupts;
mod video;
mod multitasking;
mod globals;
mod keyboard;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::File; // Trait for open/read
use uefi::proto::media::file::FileAttribute;
use uefi::proto::media::file::FileMode;

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    
    // Reset console
    system_table.stdout().reset(false).unwrap();
    
    log::info!("AetherOS Hybrid Kernel (UEFI Mode) [Step 5: Execution]");
    
    // --- Video Initialization Start ---
    let bt = system_table.boot_services();
    let gop_handle = bt.get_handle_for_protocol::<GraphicsOutput>().expect("No GOP handle found");
    let mut gop = bt.open_protocol_exclusive::<GraphicsOutput>(gop_handle).expect("Failed to open GOP");
    
    let mode_info = gop.current_mode_info();
    let (width, height) = mode_info.resolution();
    
    let mut fb = gop.frame_buffer();
    let fb_ptr = fb.as_mut_ptr();
    let size = fb.size();
    let stride = mode_info.stride();
    
    log::info!("GOP: {}x{}, Stride: {}, FB: {:p}", width, height, stride, fb_ptr);
    
    // Initialize Video Module
    crate::video::init(fb_ptr, size, width, height, stride);
    
    // Clear Screen (Blue)
    unsafe {
        let ptr = fb_ptr as *mut u32;
        let count = size / 4;
        for i in 0..count {
             // ABGR or BGRA? Usually BGRA (Blue is low byte)
             // 0x000000FF = Red? 
             // 0xFF000000 = Alpha?
             // Let's try 0xFF000080 (Dark Blue)
             *ptr.add(i) = 0xFF000080; 
        }
    }
    // --- Video Initialization End ---
    
    // Initialize IDT
    interrupts::init_idt();
    
    // --- FileSystem Loading Start ---
    log::info!("Searching for guest kernel...");
    
    let bt = system_table.boot_services();
    
    // 1. Get LoadedImage to find out which device we booted from
    let loaded_image = bt.open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(image_handle)
        .expect("Failed to open LoadedImage");
    let device_handle = loaded_image.device().expect("Device handle missing");
    
    // 2. Open FileSystem on that device
    let mut sfs = bt.open_protocol_exclusive::<uefi::proto::media::fs::SimpleFileSystem>(device_handle)
        .expect("Failed to open SimpleFileSystem");
    let mut root = sfs.open_volume().expect("Failed to open volume");
    
    // 3. Open guest file
    let filename = uefi::cstr16!("guest-x86_64.bin");
    
    // Explicit type to help inference
    let guest_image = match root.open(filename, FileMode::Read, FileAttribute::empty()) {
        Ok(file_handle) => {
             // Convert generic FileHandle to RegularFile
             let mut file = file_handle.into_regular_file().expect("Not a regular file");
             log::info!("Found guest kernel: guest-x86_64.bin");
             
             // Get file size (needs info buffer)
             // Simplified: Just seek end
             file.set_position(0xFFFFFFFFFFFFFFFF).unwrap(); 
             let size = file.get_position().unwrap();
             file.set_position(0).unwrap();
             
             log::info!("Guest Size: {} bytes", size);
             
             let mut buffer = alloc::vec![0u8; size as usize];
             file.read(&mut buffer).unwrap();
             buffer
        },
        Err(e) => {
            log::error!("Failed to open guest-x86_64.bin: {:?}", e);
            panic!("Cannot load guest kernel!");
        }
    };
    // --- FileSystem Loading End ---


 
     log::info!("Initializing Scheduler...");


    // 1. Setup Scheduler
    let mut scheduler = aether_core::scheduler::Scheduler::new();
    
    // 2. Spawn Initial Processes
    log::info!("Spawning Guest Instance 1...");
    let guest_copy = guest_image.clone();
    let backend1 = alloc::sync::Arc::new(backend::UefiBackend::new(guest_image));
    let pid1 = scheduler.spawn(backend1.clone());
    
    if let Some(proc) = scheduler.get_process_mut(pid1) {
        let entry = backend1.entry_point();
        let base = backend1.base_address();
        log::info!("Init PID {} Stack. Base: {:x}", pid1, base);
        proc.stack_pointer = multitasking::init_stack(&mut proc.stack, entry, base);
    }
    
    log::info!("Spawning Guest Instance 2...");
    let backend2 = alloc::sync::Arc::new(backend::UefiBackend::new(guest_copy));
    let pid2 = scheduler.spawn(backend2.clone());
    


    // Process 2 Stack
    if let Some(proc) = scheduler.get_process_mut(pid2) {
        let entry = backend2.entry_point();
        // Base address will be different because it's a new allocation!
        let base = backend2.base_address();
        log::info!("Init PID {} Stack. Base: {:x}", pid2, base);
        proc.stack_pointer = multitasking::init_stack(&mut proc.stack, entry, base);
    }
    
    // Initialize Global Scheduler
    {
        let mut lock = globals::SCHEDULER.lock();
        *lock = Some(scheduler);
    }
    
    // Enable Competing Interrupts (Timer)
    x86_64::instructions::interrupts::enable();
    
    log::info!("Scheduler initialized. Entering Idle Loop via Interrupts...");
    
    loop {
        x86_64::instructions::hlt();
    }
    
    // Unreachable
    // Status::SUCCESS 
}
