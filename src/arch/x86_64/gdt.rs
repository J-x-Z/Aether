//! Global Descriptor Table (GDT)
//! 
//! Custom GDT implementation with 16 entries to support:
//! - Kernel/User segments
//! - TSS (2 slots)
//! - UEFI compatibility padding (0x30, 0x38)
//! - Syscall requirements (Contiguous Code+Data)

use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::SegmentSelector;
use x86_64::{VirtAddr, PrivilegeLevel};
use spin::Lazy;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(unsafe { &raw const STACK });
        let stack_end = stack_start + STACK_SIZE;
        stack_end
    };
    tss
});

/// Custom GDT struct because x86_64 crate's GDT is fixed to 8 entries
/// We need more for UEFI layout compatibility + Syscall requirements
#[repr(C)]
pub struct CustomGdt {
    entries: [u64; 16],
    next_free: usize,
}

impl CustomGdt {
    pub const fn new() -> Self {
        CustomGdt {
            entries: [0; 16],
            next_free: 1, // Skip NULL
        }
    }

    /// Add an entry manually at a specific index
    pub fn add_entry_at(&mut self, index: usize, value: u64) -> SegmentSelector {
        assert!(index < 16, "GDT index out of bounds");
        self.entries[index] = value;
        SegmentSelector::new(index as u16, PrivilegeLevel::Ring0)
    }
    
    /// Add a 16-byte TSS descriptor (takes 2 slots)
    pub fn add_tss(&mut self, index: usize, tss: &'static TaskStateSegment) -> SegmentSelector {
        use x86_64::structures::gdt::Descriptor;
        assert!(index + 1 < 16, "GDT TSS index out of bounds");
        
        let ptr = tss as *const _ as u64;
        let limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;
        
        // Manual TSS Descriptor construction (System Segment)
        // System descriptors are 16 bytes in Long Mode.
        // Base[0-23] @ 16-39
        // Access Byte (P, DPL, Type) @ 40-47
        // Limit[16-19] + Flags @ 48-55
        // Base[24-31] @ 56-63
        
        let low = ((ptr & 0xFFFFFF) << 16)              // Base 0-23
                | ((ptr & 0xFF000000) << 32)            // Base 24-31
                | (limit & 0xFFFF)                      // Limit 0-15
                | ((limit & 0xF0000) << 32)             // Limit 16-19
                | (0x89 << 40);                         // Type=0x9, S=0, P=1, DPL=0
                
        let high = ptr >> 32;

        self.entries[index] = low;
        self.entries[index + 1] = high;
        
        SegmentSelector::new(index as u16, PrivilegeLevel::Ring0)
    }

    pub fn load(&'static self) {
        use x86_64::instructions::tables::lgdt;
        use x86_64::structures::DescriptorTablePointer;
        
        let ptr = DescriptorTablePointer {
            base: VirtAddr::from_ptr(&self.entries),
            limit: (core::mem::size_of_val(&self.entries) - 1) as u16,
        };
        
        unsafe { lgdt(&ptr); }
    }
}

// Helpers for segment bits
const COMMON_FLAGS: u64 = 
    (1 << 44) | // Descriptor Type (1 = Code/Data)
    (1 << 47);  // Present

const EXECUTABLE: u64 = 1 << 43;
const LONG_MODE: u64 = 1 << 53;
const WRITABLE: u64 = 1 << 41; // For Data
const READABLE: u64 = 1 << 41; // For Code
const USER: u64 = 1 << 45; // DPL 3

const KERNEL_CODE_VAL: u64 = COMMON_FLAGS | EXECUTABLE | LONG_MODE | READABLE;
const KERNEL_DATA_VAL: u64 = COMMON_FLAGS | WRITABLE;
const USER_CODE_VAL: u64 = COMMON_FLAGS | EXECUTABLE | LONG_MODE | READABLE | USER;
const USER_DATA_VAL: u64 = COMMON_FLAGS | WRITABLE | USER;


struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

static GDT: Lazy<(CustomGdt, Selectors)> = Lazy::new(|| {
    let mut gdt = CustomGdt::new();
    
    // Layout Plan:
    // 0: Null
    // 1: User Data (0x8)
    // 2: User Code (0x10)
    // 3: TSS Low  (0x18)
    // 4: TSS High (0x20)
    // 5: Padding  (0x28) - Kernel Data (dummy)
    // 6: Kernel Data (0x30) - Matches UEFI SS/DS
    // 7: Kernel Code (0x38) - Matches UEFI CS (and Syscall CS)
    // 8: Kernel Data (0x40) - Syscall SS
    
    let user_data_selector = gdt.add_entry_at(1, USER_DATA_VAL); // 0x8 | R3
    let user_code_selector = gdt.add_entry_at(2, USER_CODE_VAL); // 0x10 | R3
    
    // Adjust selectors to Ring 3
    let user_data_selector = SegmentSelector::new(1, PrivilegeLevel::Ring3);
    let user_code_selector = SegmentSelector::new(2, PrivilegeLevel::Ring3);

    let tss_selector = gdt.add_tss(3, &TSS); // 0x18
    
    gdt.add_entry_at(5, KERNEL_DATA_VAL); // 0x28
    
    let uefi_data_compatible = gdt.add_entry_at(6, KERNEL_DATA_VAL); // 0x30
    
    let kernel_code_selector = gdt.add_entry_at(7, KERNEL_CODE_VAL); // 0x38
    let kernel_data_selector = gdt.add_entry_at(8, KERNEL_DATA_VAL); // 0x40
    
    (gdt, Selectors {
        code_selector: kernel_code_selector,
        data_selector: kernel_data_selector,
        user_code_selector,
        user_data_selector,
        tss_selector,
    })
});

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, DS, ES, SS, FS, GS, Segment};
    
    // Ensure TSS Lazy init
    let _ = &*TSS;
    
    GDT.0.load();
    unsafe {
        // Use our new syscall-compliant selectors
        CS::set_reg(GDT.1.code_selector); // 0x38
        DS::set_reg(GDT.1.data_selector); // 0x40
        ES::set_reg(GDT.1.data_selector);
        SS::set_reg(GDT.1.data_selector);
        FS::set_reg(GDT.1.data_selector);
        GS::set_reg(GDT.1.data_selector);
        
        load_tss(GDT.1.tss_selector);
    }
    
    log::info!("[Arch] GDT and TSS initialized");
}

pub fn kernel_cs() -> u16 { GDT.1.code_selector.0 }
pub fn kernel_ds() -> u16 { GDT.1.data_selector.0 }
pub fn user_cs() -> u16 { GDT.1.user_code_selector.0 }
pub fn user_ds() -> u16 { GDT.1.user_data_selector.0 }

pub unsafe fn set_interrupt_stack(stack_top: u64) {
    let tss = &*TSS as *const TaskStateSegment as *mut TaskStateSegment;
    (*tss).privilege_stack_table[0] = VirtAddr::new(stack_top);
    log::debug!("[GDT] TSS RSP0 set to 0x{:x}", stack_top);
}
