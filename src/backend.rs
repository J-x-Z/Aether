use alloc::vec::Vec;
use aether_core::backend::{Backend, ExitReason};
use aether_abi::mmio::RAM_SIZE;

pub struct UefiBackend {
    // We hold the guest memory buffer.
    // In a real VMM, this would be mapped to a specific GPA.
    // Here we just allocate it on the heap (UEFI Pool).
    #[allow(dead_code)]
    mem: Vec<u8>,
    
    // UEFI specific handles
}

// Safety: UEFI is single-threaded in this context usually, but Backend requires Sync.
// We are mocked for now.
unsafe impl Send for UefiBackend {}
unsafe impl Sync for UefiBackend {}

impl UefiBackend {
    pub fn new(_guest_image: Vec<u8>) -> Self {
        log::info!("[Aether::UefiBackend] initializing...");
        
        // 1. Allocate Guest Memory
        let mut mem = alloc::vec![0u8; RAM_SIZE];
        log::info!("[Aether::UefiBackend] Allocated {} MB for Guest RAM", RAM_SIZE / 1024 / 1024);
        
        let guest_bin = _guest_image;
        
        if guest_bin.len() > mem.len() {
            panic!("Guest binary larger than RAM");
        }
        
        unsafe {
            // Copy guest to start of memory (Load Addr 0)
            core::ptr::copy_nonoverlapping(guest_bin.as_ptr(), mem.as_mut_ptr(), guest_bin.len());
            
            // Register Framebuffer Bridge
            // Guest writes to mem + FB_ADDR
            // We tell video module that's where the shadow buffer is.
            let fb_ptr = mem.as_ptr().add(aether_abi::mmio::FB_ADDR as usize);
            crate::video::set_guest_buffer(fb_ptr);
        }
        log::info!("[Aether::UefiBackend] Guest Loaded: {} bytes", guest_bin.len());
        
        UefiBackend {
            mem
        }
    }
    pub fn entry_point(&self) -> usize {
        self.mem.as_ptr() as usize
    }

    pub fn base_address(&self) -> usize {
        self.mem.as_ptr() as usize
    }
}

impl Backend for UefiBackend {


    fn name(&self) -> &str {
        "UEFI Bare Metal (No Virtualization)"
    }

    fn step(&self) -> ExitReason {
        // In Multi-Unikernel mode, 'step' is not used for execution.
        // Execution happens via Context Switching.
        ExitReason::Yield
    }

    unsafe fn get_framebuffer(&self, _width: usize, _height: usize) -> &[u32] {
        // Return dummy or actual FB if we had access to GOP here.
        // Without storing GOP in the struct, we can't.
        &[]
    }

    fn inject_key(&self, c: char) {
        // Write to MMIO buffer
        // KEYBOARD_STATUS = 0x80000
        // KEYBOARD_DATA = 0x80004
        // Use aether_abi constants
        unsafe {
            let status_ptr = self.mem.as_ptr().add(aether_abi::mmio::KEYBOARD_STATUS as usize) as *mut u32;
            let data_ptr = self.mem.as_ptr().add(aether_abi::mmio::KEYBOARD_DATA as usize) as *mut u32;
            
            // Only write if buffer is empty (Status == 0) to avoid overwriting?
            // Actually, we are just a simple buffer, overwriting is fine for now.
            // But real hardware has a buffer.
            
            data_ptr.write_volatile(c as u32);
            status_ptr.write_volatile(1); // Set Status = Ready
        }
    }
}
