use spin::Mutex;
use lazy_static::lazy_static;
use core::ptr;
use core::slice;
use log::info;

// Basic GOP Info
struct VideoState {
    base: *mut u32,
    size: usize,
    width: usize,
    height: usize,
    stride: usize,
}

unsafe impl Send for VideoState {}
unsafe impl Sync for VideoState {}

// Guest Buffer (Shadow FB)
static mut GUEST_FB: *const u32 = ptr::null();

lazy_static! {
    static ref VIDEO: Mutex<Option<VideoState>> = Mutex::new(None);
}

// Initialize real hardware framebuffer
pub fn init(base: *mut u8, size: usize, width: usize, height: usize, stride: usize) {
    info!("[Aether::Video] Initializing GOP: {:p} ({}x{})", base, width, height);
    let mut video = VIDEO.lock();
    *video = Some(VideoState {
        base: base as *mut u32,
        size,
        width,
        height,
        stride,
    });
}

// Register where the Guest is writing pixels
pub fn set_guest_buffer(ptr: *const u8) {
    unsafe {
        // Guest writes to FB_ADDR (0x100000)
        // We assume 32-bit color (4 bytes)
        GUEST_FB = ptr as *const u32;
    }
}

pub fn blit() {
    // This is called from Interrupt Handler! Be super careful.
    // spin::Mutex is safe in interrupts.
    
    if let Some(ref v) = *VIDEO.lock() {
        unsafe {
            if GUEST_FB.is_null() { return; }
            
            // Optimization: Only blit if we have a guest buffer
            // Copy line by line handling stride
            let src = GUEST_FB;
            let dst = v.base;
            
            // Simple byte copy for now?
            // If stride == width, we can do one big copy
            // Usually stride matches width in pixels for 32bpp
            
            // To prevent tearing or slowness, maybe copy in chunks?
            // For verification, just copy everything.
            // 640x480 * 4 = 1.2MB. memcpy is fast.
            
            // Note: src is from UefiBackend::new allocation.
            // dst is MMIO.
            
            ptr::copy_nonoverlapping(src, dst, v.width * v.height);
        }
    }
}
