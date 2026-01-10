use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use pic8259::ChainedPics;
use spin::Mutex;
use log::{info, error};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
    Serial1 = PIC_1_OFFSET + 4,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

use spin::Once;
use core::sync::atomic::{AtomicU64, Ordering};

/// System Ticks (Timer Interrupts)
pub static TICKS: AtomicU64 = AtomicU64::new(0);

static IDT: Once<InterruptDescriptorTable> = Once::new();

fn create_idt() -> InterruptDescriptorTable {
    use x86_64::instructions::segmentation::{CS, Segment};
    let current_cs = CS::get_reg();
    info!("[Interrupts] Creating IDT with CS: {:?} (Expected: 0x8)", current_cs);
    
    let mut idt = InterruptDescriptorTable::new();
    
    // 1. Set default handler for ALL 256 entries to catch stray interrupts
    // UEFI/Hardware might fire APIC/MSI interrupts we didn't expect (e.g. Vector 0x6E)
    for i in 0..256 {
        idt[i].set_handler_fn(default_interrupt_handler);
    }

    idt.breakpoint.set_handler_fn(breakpoint_handler);
    unsafe {
        idt.double_fault.set_handler_fn(double_fault_handler)
            // Use a dedicated stack for Double Faults to prevent Triple Faults
            .set_stack_index(crate::arch::x86_64::gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
    idt.segment_not_present.set_handler_fn(segment_not_present_handler);
    
    // Timer Interrupt
    idt[InterruptIndex::Timer.as_usize()]
        .set_handler_fn(timer_interrupt_handler);
    idt[InterruptIndex::Keyboard.as_usize()]
        .set_handler_fn(keyboard_interrupt_handler);
    idt[InterruptIndex::Serial1.as_usize()]
        .set_handler_fn(serial_interrupt_handler);

    idt
}

extern "x86-interrupt" fn default_interrupt_handler(stack_frame: InterruptStackFrame) {
    // Just log it and return (ignore spurious interrupts)
    // Don't panic, or we'll loop if it's high frequency
    // Use early_serial_print as log info might be too slow/buffered
    // But screen_print is better visible
    // Wait, we can't easily print the vector number here because x86-interrupt calling convention
    // doesn't pass the vector number to the handler (except for error codes).
    // But at least we won't crash.
    // We can use a trick or just print generic message.
    
    // WARNING: This handler is for debug. Ideally we should ACK EOI if it was from APIC.
    // If we don't ACK, it might just fire once and stop, or hang the APIC.
    // For now, let's see if we survive.
    // x86_64 crate's set_handler_fn expects `fn(ISF)`.
    
    // Try to signal End of Interrupt to both PICs just in case,
    // although if it's Vector 0x6E it's arguably NOT a PIC interrupt.
    // If it's APIC, we need local APIC EOI.
    
    // Minimal output to avoid blocking
    // unsafe { crate::main::early_serial_print(b"![IRQ]\r\n"); }
}

// ... init_pit ...

// ...



// PIT defaults to 18.2Hz if untouched, but we want faster checks for UI.
// Let's set it to ~100Hz.
pub fn init_pit() {
    let mut command_port = x86_64::instructions::port::Port::<u8>::new(0x43);
    let mut data_port = x86_64::instructions::port::Port::<u8>::new(0x40);
    
    // 0x34: Channel 0, Lo/Hi Byte, Rate Generator (Mode 2), Binary
    unsafe { command_port.write(0x34) };
    
    // 1193182 / 100 Hz = 11931
    let divisor = 11931u16;
    unsafe {
        data_port.write((divisor & 0xFF) as u8);
        data_port.write((divisor >> 8) as u8);
    }
}

use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::structures::gdt::SegmentSelector;

pub fn init_idt() {
    info!("[Aether::Interrupts] Initializing IDT...");
    
    // CRITICAL FIX: Force CS to correct Kernel Code Selector (0x38)
    // 0x08 is User Data in our new GDT!
    unsafe { 
        CS::set_reg(SegmentSelector(crate::arch::x86_64::gdt::kernel_cs())); 
    }
    
    let idt = IDT.call_once(create_idt);
    idt.load();
    info!("[Aether::Interrupts] IDT loaded");
    
    unsafe { 
        let mut pics = PICS.lock();
        pics.initialize();
        info!("[Aether::Interrupts] PICS initialized");
        
        // Manual Unmasking of IRQ 1 (Keyboard) and IRQ 4 (Serial)
        let mut master_data = x86_64::instructions::port::Port::<u8>::new(0x21);
        let mask = master_data.read();
        let new_mask = mask & !( (1 << 1) | (1 << 4) );
        master_data.write(new_mask);
    }
    info!("[Aether::Interrupts] IRQs unmasked");

    init_pit();
    info!("[Aether::Interrupts] PIT initialized");
    
    // NOW it's safe: Our GDT is loaded, our IDT is loaded with correct CS, PICs are configured.
    // Re-enable interrupts.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    info!("[Aether::Interrupts] IDT ready, interrupts enabled.");
}

extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame)
{
    info!("[EXCEPTION] BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    panic!("[EXCEPTION] DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    log::error!("[EXCEPTION] PAGE FAULT");
    log::error!("Accessed Address: {:?}", Cr2::read());
    log::error!("Error Code: {:?}", error_code);
    log::error!("{:#?}", stack_frame);
    panic!("Page Fault");
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    log::error!("[EXCEPTION] GENERAL PROTECTION FAULT\nError Code: {}\n{:#?}", error_code, stack_frame);
    panic!("GPF");
}

extern "x86-interrupt" fn invalid_opcode_handler(
    stack_frame: InterruptStackFrame) 
{
    log::error!("[EXCEPTION] INVALID OPCODE\n{:#?}", stack_frame);
    panic!("Invalid Opcode");
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64) 
{
    log::error!("[EXCEPTION] STACK FAULT\nError: {}\n{:#?}", error_code, stack_frame);
    panic!("Stack Fault");
}

extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    log::error!("[EXCEPTION] SEGMENT NOT PRESENT\nError: {}\n{:#?}", error_code, stack_frame);
    panic!("Segment Not Present");
}

extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    use x86_64::instructions::port::Port;
    
    // 1. Read Scancode
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    
    // 2. Process Scancode
    if let Some(key) = crate::keyboard::process_scancode(scancode) {
        // PUSH TO GLOBAL BUFFER
        crate::drivers::console_input::push_char(key);
        
        // OPTIONAL: Local Echo for Debugging (Print to screen immediately)
        // print!("{}", key); 

        // 3. Inject into Guests (Multi-Cast) - Optional for now
        if let Some(mut sched_lock) = crate::globals::SCHEDULER.try_lock() {
            if let Some(sched) = (*sched_lock).as_mut() {
                for process in &sched.processes {
                    process.backend.inject_key(key);
                }
            }
        }
    }

    // Safety: we must notify EOI
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn serial_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Read from Serial Port
    if let Some(byte) = crate::drivers::console::read_serial() {
         // Push to global buffer
         crate::drivers::console_input::push_char(byte as char);
         // Local echo optional
         // print!("{}", byte as char);
    }
    
    // Notify PICS
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Serial1.as_u8());
    }
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame) 
{
    // Increment Tick Counter
    TICKS.fetch_add(1, Ordering::Relaxed);
    
    // Check for Sleeping Tasks
    crate::sched::check_timers();

    // Blit Shadow Buffer to Screen
    crate::video::blit();

    // Preemptive Multitasking
    // Try to lock scheduler
    if let Some(mut sched_lock) = crate::globals::SCHEDULER.try_lock() {
        if let Some(sched) = (*sched_lock).as_mut() {
            let prev_pid = sched.current_pid;
            
            // Check if we need to switch
            if let Some(next_pid) = sched.schedule() {
                
                // 1. Resolve Old Stack Pointer location
                // If prev_pid is None or invalid, we save to IDLE/BOOT stack.
                let old_sp_ptr = match prev_pid {
                    Some(pid) => {
                        if let Some(p) = sched.get_process_mut(pid) {
                            &mut p.stack_pointer as *mut usize
                        } else {
                             unsafe { &mut crate::globals::IDLE_STACK_POINTER as *mut usize }
                        }
                    },
                    None => unsafe { &mut crate::globals::IDLE_STACK_POINTER as *mut usize }
                };
                
                // 2. Resolve New Stack Pointer & Kernel Stack Top
                // We know next_pid is valid because schedule returned it
                let proc = sched.get_process_mut(next_pid).unwrap();
                let new_sp = proc.stack_pointer;
                
                // Calculate Kernel Stack Top (High Address) for TSS
                // This ensures interrupts from Ring 3 use this process's kernel stack
                let kernel_stack_top = proc.stack.as_ptr() as u64 + proc.stack.len() as u64;

                // log::trace!("[Timer] Switching {:?} -> {} (TSS.RSP0={:x})", prev_pid, next_pid, kernel_stack_top);

                // UPDATE TSS RSP0!
                unsafe {
                    crate::arch::x86_64::gdt::set_interrupt_stack(kernel_stack_top);
                }

                // Release lock before switch!
                drop(sched_lock);
                
                // 3. Switch Context
                unsafe {
                    crate::multitasking::switch_context(new_sp, old_sp_ptr);
                }
            }
        }
    }

    // Safety: we must notify EOI
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}
