//! Global Descriptor Table (GDT)
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
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

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    
    // Kernel Ring 0
    let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
    let data_selector = gdt.add_entry(Descriptor::kernel_data_segment());
    
    // User Ring 3
    let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
    let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());
    
    // TSS
    let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
    
    (gdt, Selectors {
        code_selector,
        data_selector,
        user_code_selector,
        user_data_selector,
        tss_selector,
    })
});

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, DS, ES, SS, FS, GS, Segment};
    
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        ES::set_reg(GDT.1.data_selector);
        SS::set_reg(GDT.1.data_selector);
        FS::set_reg(GDT.1.data_selector); // Or user segment if needed
        GS::set_reg(GDT.1.data_selector);
        
        load_tss(GDT.1.tss_selector);
    }
    
    log::info!("[Arch] GDT and TSS initialized (Ring 0 & 3 support)");
}

/// Get Kernel Code Selector
pub fn kernel_cs() -> u16 {
    GDT.1.code_selector.0
}

/// Get Kernel Data Selector
pub fn kernel_ds() -> u16 {
    GDT.1.data_selector.0
}

/// Get User Code Selector
pub fn user_cs() -> u16 {
    GDT.1.user_code_selector.0
}

/// Get User Data Selector
pub fn user_ds() -> u16 {
    GDT.1.user_data_selector.0
}
