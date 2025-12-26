use core::arch::global_asm;

// Assembly for Context Switching
// arguments: rdi = new_stack_pointer, rsi = old_stack_pointer_ptr
global_asm!(r#"
.global switch_context
switch_context:
    // 1. Save Callee-Saved Registers
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    
    // 2. Save current RSP to old_stack_pointer_ptr (*rsi)
    mov [rsi], rsp
    
    // 3. Load new RSP from new_stack_pointer (rdi)
    mov rsp, rdi
    
    // 4. Restore Callee-Saved Registers from new stack
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    
    // 5. Return (pops RIP)
    ret
"#);

extern "C" {
    pub fn switch_context(new_sp: usize, old_sp_ptr: *mut usize);
}

// Function that newly created threads "return" to.
// This function mimics the end of an interrupt handler.
// It pops the TrapFrame and Irets.
// But wait, if we are just starting, we can just call the function?
// In strict preemption, we want `iretq` to enable interrupts atomically with jump.
// So we should construct a TrapFrame.

use x86_64::structures::idt::InterruptStackFrameValue;
use core::mem::size_of;

/// Initialize a process stack
pub fn init_stack(stack: &mut [u8], entry_point: usize, arg0: usize) -> usize {
    let stack_top = stack.as_ptr() as usize + stack.len();
    
    // We need to align stack to 16 bytes
    let mut sp = stack_top & !0xF;
    
    unsafe {
        let ptr = sp as *mut u8;
        
        // 1. Trap Context (setup for iretq)
        // Layout: SS, RSP, RFLAGS, CS, RIP
        sp -= 8; *ptr.add(sp - stack.as_ptr() as usize).cast::<u64>() = 0x0; // SS (0 on x86_64 mostly ignored or handled by iret?) 
                                                                             // Actually in 64-bit mode, SS/CS are specific selectors.
                                                                             // We need valid Selectors. UEFI CS=0x38?
        // This is getting tricky because we need valid CS/SS from GDT.
        // UEFI GDT is dynamic.
        // Alternative: New threads start by `call function`.
        // Only *interrupted* threads have TrapFrame.
        // So `switch_context` returns to `trampoline` which calls `entry(arg)`.
        
        // Let's use the Trampoline approach for simplicity.
        // Stack: [Trampoline Addr, RBX, RBP, R12, R13, R14, R15]
        // When switch_context pops everything and rets, it rets to Trampoline.
        // Trampoline calls `entry(arg)`.
        
        // Push Switch Context
        sp -= 8; *ptr.add(sp - stack.as_ptr() as usize).cast::<usize>() = trampoline as usize; // Return Address (RIP)
        
        // Push R15..RBX (6 registers)
        sp -= 8 * 6;
        
        // We can pass `entry_point` and `arg0` via registers R12, R13?
        // System V ABI: rbx, rbp, r12-r15 are callee saved.
        // We can use them to store initial state.
        
        // Let's store `entry_point` in R12, `arg0` in R13.
        let regs_ptr = ptr.add(sp - stack.as_ptr() as usize).cast::<usize>();
        // Stack order: RBX, RBP, R12, R13, R14, R15 (Pushed order)
        // Wait, pop order is reverse: R15, R14, R13, R12, RBP, RBX.
        // switch_context pops R15 first.
        // So memory order (low to high): R15 ... RBX.
        // No, PUSH decrements SP.
        // Pushing: RBX (high), ..., R15 (low).
        // Popping: R15 (low), ..., RBX (high).
        
        // So at `sp`: R15
        *regs_ptr.add(0) = 0; // R15
        *regs_ptr.add(1) = 0; // R14
        *regs_ptr.add(2) = arg0; // R13
        *regs_ptr.add(3) = entry_point; // R12
        *regs_ptr.add(4) = 0; // RBP
        *regs_ptr.add(5) = 0; // RBX
    }
    
    sp
}

#[no_mangle]
extern "C" fn trampoline() -> ! {
    // We are now running on the new stack!
    // Recover arguments from R12, R13 (which were restored by switch_context)
    let entry: extern "C" fn(usize) -> !;
    let arg: usize;
    
    unsafe {
        core::arch::asm!(
            "mov {0}, r12",
            "mov {1}, r13",
            out(reg) entry,
            out(reg) arg
        );
        
        // Create a proper stack frame for backtraces?
        // x86_64::instructions::interrupts::enable();
        
        entry(arg);
    }
}
