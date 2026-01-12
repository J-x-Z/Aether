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

// ============================================================================
// Simple Framebuffer Console
// ============================================================================

struct ConsoleState {
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u32,
    bg_color: u32,
    ansi_state: u8,
}

static EXT_CONSOLE: Mutex<ConsoleState> = Mutex::new(ConsoleState {
    cursor_x: 0,
    cursor_y: 0,
    fg_color: 0xFFFFFFFF, // White (Opaque)
    bg_color: 0xFF000000, // Black (Opaque)
    ansi_state: 0,
});

pub fn draw_pixel(x: usize, y: usize, color: u32) {
    if let Some(ref mut v) = *VIDEO.lock() {
        if x < v.width && y < v.height {
            unsafe {
                *v.base.add(y * v.stride + x) = color;
            }
        }
    }
}

pub fn draw_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    if let Some(ref mut v) = *VIDEO.lock() {
        for row in 0..h {
            for col in 0..w {
                if x + col < v.width && y + row < v.height {
                     unsafe { *v.base.add((y + row) * v.stride + (x + col)) = color; }
                }
            }
        }
    }
}

pub fn draw_char(x: usize, y: usize, c: char, fg: u32) {
    // Only target x86_64 for now as font is x86_64 only
    #[cfg(target_arch = "x86_64")]
    {
        let glyph = crate::font::get_glyph(c);
        for row in 0..8 {
            let byte = glyph[row];
            for col in 0..8 {
                if (byte >> (7 - col)) & 1 != 0 {
                    draw_pixel(x + col, y + row, fg);
                }
            }
        }
    }
}

pub fn console_print_char(byte: u8) {
    // Basic terminal emulator logic
    let mut console = EXT_CONSOLE.lock();
    let width = if let Some(ref v) = *VIDEO.lock() { v.width } else { 800 };
    let height = if let Some(ref v) = *VIDEO.lock() { v.height } else { 600 };
    
    // ANSI Escape Sequence Parser
    match console.ansi_state {
        1 => { // Seen ESC
            if byte == b'[' {
                console.ansi_state = 2; // CSI Mode
            } else {
                console.ansi_state = 0; // Invalid/Unsupported ESC sequence, drop ESC, handle this byte as normal?
                // Ideally we handle other ESC sequences too, but for BusyBox sh, CSI is main one.
                // If we get here, we consumed ESC. If this byte is 'c' (Reset), we ignore.
                // If it's a printable char, we should probably print it?
                // Let's just reset and fall through only if printable?
                // For safety/simplicity, just reset to 0 and ignore this byte if it was control?
                // Let's re-eval byte in state 0?
                // Recursion complex. Just reset.
            }
            return;
        }
        2 => { // Inside CSI (params)
            // 0x30-0x3F (0-9;?) are params.
            // 0x40-0x7E are Dispatch info (Final byte).
            if byte >= 0x40 && byte <= 0x7E {
                // Command Byte!
                // Implement basic cursor moves logic if we want "EJ" to actually work?
                // J = Erase Display, K = Erase Line.
                // User complained about "EJ". Probably "ESC [ J".
                // We should handle them if easy.
                match byte {
                    b'J' => { /* Erase Display - simplified: clear screen? */ }
                    b'K' => { 
                         // Erase Line (from cursor to end)
                         // draw_rect(console.cursor_x, cursor_y, width - cursor_x, 16, bg)
                         let remaining = if width > console.cursor_x { width - console.cursor_x } else { 0 };
                         draw_rect(console.cursor_x, console.cursor_y, remaining, 16, console.bg_color);
                    }
                    _ => {}
                }
                console.ansi_state = 0; // End of sequence
            }
            // Else stay in state 2 (swallowing params)
            return;
        }
        _ => {}
    }
    
    // Normal handling
    if byte == 0x1B { // ESC
        console.ansi_state = 1;
        return;
    }

    match byte {
        b'\n' => {
            console.cursor_x = 0;
            console.cursor_y += 16; // Line height 16 (padding)
        }
        b'\r' => {
            console.cursor_x = 0;
        }
        b'\x08' | 0x7F => { // Backspace or DEL
             if console.cursor_x >= 8 {
                 console.cursor_x -= 8;
                 // DESTRUCTIVE BACKSPACE:
                 // Immediately clear the character we just moved over.
                 draw_rect(console.cursor_x, console.cursor_y, 8, 16, console.bg_color);
             }
        }
        _ => {
            // Only draw printable characters
            if byte >= 0x20 && byte != 0x7F {
                // If Space (0x20), draw opaque rectangle to clear background
                if byte == 0x20 {
                    draw_rect(console.cursor_x, console.cursor_y, 8, 16, console.bg_color);
                } else {
                    // For other characters, assume font is transparent? 
                    // To prevent overlap if we are overwriting, we SHOULD clear the cell first.
                    // Let's clear the cell for ALL printable characters.
                    // This ensures clean rendering.
                    draw_rect(console.cursor_x, console.cursor_y, 8, 16, console.bg_color);
                    draw_char(console.cursor_x, console.cursor_y, byte as char, console.fg_color);
                }
                console.cursor_x += 8;
            }
        }
    }
    
    // Wrap
    if console.cursor_x >= width {
        console.cursor_x = 0;
        console.cursor_y += 16;
    }
    
    // Scroll (Wrap around to top clear for now, implementation of scroll is expensive without blit)
    if console.cursor_y + 16 >= height {
        console.cursor_y = 0;
        // Optionally clear screen? For now just overwrite loop
    }
}
