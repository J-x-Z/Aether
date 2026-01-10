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

/// Draw a colored bar on the framebuffer for panic/debug indication
/// Works even when UEFI services are unavailable (after GDT switch)
/// color: 0=Red, 1=Green, 2=Blue, 3=Yellow, 4=Magenta, 5=Cyan, 6=White
pub fn panic_fb(color_index: u8, bar_index: u8) {
    if let Some(ref v) = *VIDEO.lock() {
        let color = match color_index {
            0 => 0x00FF0000u32, // Red
            1 => 0x0000FF00u32, // Green
            2 => 0x000000FFu32, // Blue
            3 => 0x00FFFF00u32, // Yellow
            4 => 0x00FF00FFu32, // Magenta
            5 => 0x0000FFFFu32, // Cyan
            _ => 0x00FFFFFFu32, // White
        };
        
        let bar_height = 50;
        let y_start = (bar_index as usize) * bar_height;
        let y_end = core::cmp::min(y_start + bar_height, v.height);
        
        unsafe {
            for y in y_start..y_end {
                for x in 0..v.width {
                    let offset = y * v.stride + x;
                    *v.base.add(offset) = color;
                }
            }
        }
    }
}

/// Draw panic info with multiple colored bars
/// Pattern: Red-Green-Blue = Page Fault, Red-Red-Blue = GPF, etc.
pub fn panic_pattern(colors: &[u8]) {
    for (i, &c) in colors.iter().enumerate() {
        panic_fb(c, i as u8);
    }
}

