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
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);
        
        // Timer Interrupt
        idt[InterruptIndex::Timer.as_usize()]
            .set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()]
            .set_handler_fn(keyboard_interrupt_handler);
            
        idt
    };
}

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

pub fn init_idt() {
    info!("[Aether::Interrupts] Initializing IDT...");
    IDT.load();
    unsafe { PICS.lock().initialize() };
    init_pit();
    // Enable interrupts in Main, not here, to avoid premature ticks.
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

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    error!("[EXCEPTION] GENERAL PROTECTION FAULT\nError Code: {}\n{:#?}", error_code, stack_frame);
    panic!("GPF");
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
        // 3. Inject into Guests (Multi-Cast)
        if let Some(mut sched_lock) = crate::globals::SCHEDULER.try_lock() {
            if let Some(sched) = (*sched_lock).as_mut() {
                // Broadcast input to all processes!
                // Ideally we only send to "Focused" process, but for now we broadcast.
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

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame) 
{
    // Blit Shadow Buffer to Screen
    crate::video::blit();

    // Preemptive Multitasking
    // Try to lock scheduler
    if let Some(mut sched_lock) = crate::globals::SCHEDULER.try_lock() {
        if let Some(sched) = (*sched_lock).as_mut() {
            let prev_pid = sched.current_pid;
            
            // Check if we need to switch
            // Ensure we don't switch if we are in the middle of crucial kernel things?
            // "schedule()" handles the decision.
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
                
                // 2. Resolve New Stack Pointer
                // Unwrap is safe because schedule() returned valid PID
                let new_sp = sched.get_process_mut(next_pid).unwrap().stack_pointer;
                
                log::trace!("[Timer] Switching {:?} -> {}", prev_pid, next_pid);

                // Release lock before switch!
                drop(sched_lock);
                
                // 3. Switch Context
                unsafe {
                    crate::multitasking::switch_context(new_sp, old_sp_ptr);
                }
            }
        }
    }

    // Safety: we must notify EOI or system hangs
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}
