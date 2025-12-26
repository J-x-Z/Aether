//! ELF Loader for execve
//!
//! Parses ELF64 binaries and loads them into memory for execution.

use alloc::vec::Vec;

/// ELF64 Header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],      // ELF identification
    pub e_type: u16,            // Object file type
    pub e_machine: u16,         // Machine type
    pub e_version: u32,         // Object file version
    pub e_entry: u64,           // Entry point address
    pub e_phoff: u64,           // Program header offset
    pub e_shoff: u64,           // Section header offset
    pub e_flags: u32,           // Processor-specific flags
    pub e_ehsize: u16,          // ELF header size
    pub e_phentsize: u16,       // Size of program header entry
    pub e_phnum: u16,           // Number of program header entries
    pub e_shentsize: u16,       // Size of section header entry
    pub e_shnum: u16,           // Number of section header entries
    pub e_shstrndx: u16,        // Section name string table index
}

/// ELF64 Program Header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub p_type: u32,            // Segment type
    pub p_flags: u32,           // Segment flags
    pub p_offset: u64,          // Offset in file
    pub p_vaddr: u64,           // Virtual address in memory
    pub p_paddr: u64,           // Physical address (ignored)
    pub p_filesz: u64,          // Size of segment in file
    pub p_memsz: u64,           // Size of segment in memory
    pub p_align: u64,           // Alignment
}

// ELF constants
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
pub const PT_LOAD: u32 = 1;
pub const PT_INTERP: u32 = 3;

/// Loaded ELF info
pub struct LoadedElf {
    pub entry_point: u64,
    pub segments: Vec<LoadedSegment>,
}

pub struct LoadedSegment {
    pub vaddr: u64,
    pub size: u64,
}

/// Parse and load ELF from buffer
pub fn load_elf(data: &[u8]) -> Result<LoadedElf, &'static str> {
    if data.len() < core::mem::size_of::<Elf64Header>() {
        return Err("Data too small for ELF header");
    }
    
    // Parse header
    let header = unsafe {
        core::ptr::read(data.as_ptr() as *const Elf64Header)
    };
    
    // Verify magic
    if header.e_ident[0..4] != ELF_MAGIC {
        return Err("Invalid ELF magic");
    }
    
    // Check 64-bit
    if header.e_ident[4] != 2 {
        return Err("Not a 64-bit ELF");
    }
    
    log::info!("[ELF] Entry point: 0x{:x}", header.e_entry);
    log::info!("[ELF] {} program headers at offset 0x{:x}", header.e_phnum, header.e_phoff);
    
    let mut segments = Vec::new();
    
    // Load program headers
    for i in 0..header.e_phnum {
        let phdr_offset = header.e_phoff as usize + (i as usize * header.e_phentsize as usize);
        
        if phdr_offset + core::mem::size_of::<Elf64Phdr>() > data.len() {
            return Err("Program header out of bounds");
        }
        
        let phdr = unsafe {
            core::ptr::read(data.as_ptr().add(phdr_offset) as *const Elf64Phdr)
        };
        
        if phdr.p_type == PT_LOAD {
            log::info!(
                "[ELF] LOAD: vaddr=0x{:x}, filesz={}, memsz={}", 
                phdr.p_vaddr, phdr.p_filesz, phdr.p_memsz
            );
            
            // Map memory region
            crate::mm::paging::make_user_accessible(phdr.p_vaddr, phdr.p_memsz);
            
            // Copy segment data
            let src = &data[phdr.p_offset as usize..(phdr.p_offset + phdr.p_filesz) as usize];
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    phdr.p_vaddr as *mut u8,
                    phdr.p_filesz as usize
                );
                
                // Zero BSS (memsz > filesz)
                if phdr.p_memsz > phdr.p_filesz {
                    let bss_start = (phdr.p_vaddr + phdr.p_filesz) as *mut u8;
                    let bss_size = (phdr.p_memsz - phdr.p_filesz) as usize;
                    core::ptr::write_bytes(bss_start, 0, bss_size);
                }
            }
            
            segments.push(LoadedSegment {
                vaddr: phdr.p_vaddr,
                size: phdr.p_memsz,
            });
        }
    }
    
    Ok(LoadedElf {
        entry_point: header.e_entry,
        segments,
    })
}

/// Set up user stack with argv and envp
/// Returns stack pointer
pub fn setup_user_stack(stack_top: u64, argv: &[&[u8]], envp: &[&[u8]]) -> u64 {
    // Stack layout (growing down):
    // [strings...]
    // [null] <- envp terminator
    // [envp[n]]
    // ...
    // [envp[0]]
    // [null] <- argv terminator
    // [argv[n]]
    // ...
    // [argv[0]]
    // [argc]
    
    let mut sp = stack_top;
    
    // First, copy all strings and collect pointers
    let mut argv_ptrs: Vec<u64> = Vec::new();
    let mut envp_ptrs: Vec<u64> = Vec::new();
    
    // Copy envp strings (reverse order)
    for env in envp.iter().rev() {
        sp -= env.len() as u64 + 1; // +1 for null terminator
        sp &= !0xF; // Align
        unsafe {
            core::ptr::copy_nonoverlapping(env.as_ptr(), sp as *mut u8, env.len());
            *((sp + env.len() as u64) as *mut u8) = 0;
        }
        envp_ptrs.insert(0, sp);
    }
    
    // Copy argv strings (reverse order)
    for arg in argv.iter().rev() {
        sp -= arg.len() as u64 + 1;
        sp &= !0xF;
        unsafe {
            core::ptr::copy_nonoverlapping(arg.as_ptr(), sp as *mut u8, arg.len());
            *((sp + arg.len() as u64) as *mut u8) = 0;
        }
        argv_ptrs.insert(0, sp);
    }
    
    // Align stack to 16 bytes
    sp &= !0xF;
    
    // Push null terminator for envp
    sp -= 8;
    unsafe { *(sp as *mut u64) = 0; }
    
    // Push envp pointers
    for ptr in envp_ptrs.iter().rev() {
        sp -= 8;
        unsafe { *(sp as *mut u64) = *ptr; }
    }
    
    // Push null terminator for argv
    sp -= 8;
    unsafe { *(sp as *mut u64) = 0; }
    
    // Push argv pointers
    for ptr in argv_ptrs.iter().rev() {
        sp -= 8;
        unsafe { *(sp as *mut u64) = *ptr; }
    }
    
    // Push argc
    sp -= 8;
    unsafe { *(sp as *mut u64) = argv.len() as u64; }
    
    sp
}
