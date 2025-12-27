//! ELF Dynamic Linker
//!
//! Implements the ld-linux.so functionality for loading dynamically linked ELF binaries.
//! This is required for glibc-linked binaries.
//!
//! ELF Dynamic Linking Process:
//! 1. Parse PT_INTERP to find the dynamic linker path
//! 2. Load the main executable and all shared libraries
//! 3. Process PT_DYNAMIC section for relocation info
//! 4. Resolve symbols via DT_SYMTAB, DT_STRTAB
//! 5. Apply relocations (RELATIVE, GLOB_DAT, JUMP_SLOT)
//! 6. Call .init sections, then transfer to _start

use alloc::vec::Vec;
use alloc::string::String;

/// Dynamic section entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Dyn {
    pub d_tag: i64,
    pub d_val: u64,
}

/// Symbol table entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Sym {
    pub st_name: u32,      // Symbol name offset in string table
    pub st_info: u8,       // Symbol type and binding
    pub st_other: u8,      // Reserved
    pub st_shndx: u16,     // Section header index
    pub st_value: u64,     // Symbol value (address)
    pub st_size: u64,      // Symbol size
}

/// Relocation entry (with addend)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Rela {
    pub r_offset: u64,     // Virtual address
    pub r_info: u64,       // Relocation type and symbol index
    pub r_addend: i64,     // Addend
}

// Dynamic section tags
pub const DT_NULL: i64 = 0;
pub const DT_NEEDED: i64 = 1;      // Name of needed library
pub const DT_PLTRELSZ: i64 = 2;    // Size of PLT relocs
pub const DT_PLTGOT: i64 = 3;      // Address of PLT/GOT
pub const DT_HASH: i64 = 4;        // Address of symbol hash table
pub const DT_STRTAB: i64 = 5;      // Address of string table
pub const DT_SYMTAB: i64 = 6;      // Address of symbol table
pub const DT_RELA: i64 = 7;        // Address of Rela relocs
pub const DT_RELASZ: i64 = 8;      // Total size of Rela relocs
pub const DT_RELAENT: i64 = 9;     // Size of one Rela reloc
pub const DT_STRSZ: i64 = 10;      // Size of string table
pub const DT_SYMENT: i64 = 11;     // Size of one symbol entry
pub const DT_INIT: i64 = 12;       // Address of init function
pub const DT_FINI: i64 = 13;       // Address of termination function
pub const DT_JMPREL: i64 = 23;     // Address of PLT relocs

// Relocation types (x86_64)
pub const R_X86_64_NONE: u32 = 0;
pub const R_X86_64_64: u32 = 1;        // Direct 64-bit
pub const R_X86_64_GLOB_DAT: u32 = 6;  // Create GOT entry
pub const R_X86_64_JUMP_SLOT: u32 = 7; // Create PLT entry
pub const R_X86_64_RELATIVE: u32 = 8;  // Adjust by program base

/// Loaded shared library info
pub struct LoadedLibrary {
    pub name: String,
    pub base_addr: u64,
    pub symtab: u64,
    pub strtab: u64,
    pub rela: u64,
    pub relasz: usize,
    pub jmprel: u64,
    pub pltrelsz: usize,
    pub init: u64,
}

/// Parse PT_DYNAMIC section and extract tables
pub fn parse_dynamic(base_addr: u64, dyn_addr: u64) -> Option<LoadedLibrary> {
    let mut lib = LoadedLibrary {
        name: String::from("main"),
        base_addr,
        symtab: 0,
        strtab: 0,
        rela: 0,
        relasz: 0,
        jmprel: 0,
        pltrelsz: 0,
        init: 0,
    };
    
    let mut ptr = dyn_addr as *const Elf64Dyn;
    
    unsafe {
        loop {
            let dyn_entry = *ptr;
            
            if dyn_entry.d_tag == DT_NULL {
                break;
            }
            
            match dyn_entry.d_tag {
                DT_STRTAB => lib.strtab = dyn_entry.d_val,
                DT_SYMTAB => lib.symtab = dyn_entry.d_val,
                DT_RELA => lib.rela = dyn_entry.d_val,
                DT_RELASZ => lib.relasz = dyn_entry.d_val as usize,
                DT_JMPREL => lib.jmprel = dyn_entry.d_val,
                DT_PLTRELSZ => lib.pltrelsz = dyn_entry.d_val as usize,
                DT_INIT => lib.init = dyn_entry.d_val,
                DT_NEEDED => {
                    // Would need to load this library
                    log::debug!("[dynlink] Needed library at strtab offset {}", dyn_entry.d_val);
                }
                _ => {}
            }
            
            ptr = ptr.add(1);
        }
    }
    
    log::info!("[dynlink] Parsed dynamic: symtab=0x{:x}, strtab=0x{:x}", lib.symtab, lib.strtab);
    
    Some(lib)
}

/// Apply relocations to loaded library
pub fn apply_relocations(lib: &LoadedLibrary) {
    log::info!("[dynlink] Applying {} bytes of relocations", lib.relasz);
    
    // Apply RELA relocations
    if lib.rela != 0 && lib.relasz > 0 {
        let num_relas = lib.relasz / core::mem::size_of::<Elf64Rela>();
        
        for i in 0..num_relas {
            let rela = unsafe {
                *((lib.rela + (i * core::mem::size_of::<Elf64Rela>()) as u64) as *const Elf64Rela)
            };
            
            apply_relocation(lib, &rela);
        }
    }
    
    // Apply PLT/GOT relocations (JMPREL)
    if lib.jmprel != 0 && lib.pltrelsz > 0 {
        let num_jmprels = lib.pltrelsz / core::mem::size_of::<Elf64Rela>();
        
        for i in 0..num_jmprels {
            let rela = unsafe {
                *((lib.jmprel + (i * core::mem::size_of::<Elf64Rela>()) as u64) as *const Elf64Rela)
            };
            
            apply_relocation(lib, &rela);
        }
    }
}

fn apply_relocation(lib: &LoadedLibrary, rela: &Elf64Rela) {
    let r_type = (rela.r_info & 0xFFFFFFFF) as u32;
    let r_sym = (rela.r_info >> 32) as usize;
    
    let addr = (lib.base_addr + rela.r_offset) as *mut u64;
    
    match r_type {
        R_X86_64_RELATIVE => {
            // B + A (base + addend)
            let value = lib.base_addr.wrapping_add(rela.r_addend as u64);
            unsafe { *addr = value; }
            log::debug!("[dynlink] RELATIVE @ 0x{:x} = 0x{:x}", rela.r_offset, value);
        }
        R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
            // Symbol resolution needed
            if lib.symtab != 0 {
                let sym = unsafe {
                    *((lib.symtab + (r_sym * core::mem::size_of::<Elf64Sym>()) as u64) as *const Elf64Sym)
                };
                
                // Get symbol name from string table
                let sym_name = if lib.strtab != 0 {
                    get_string(lib.strtab, sym.st_name as usize)
                } else {
                    String::from("<??>")
                };
                
                // If symbol is defined in this library, use its value
                if sym.st_value != 0 {
                    let value = lib.base_addr + sym.st_value;
                    unsafe { *addr = value; }
                    log::debug!("[dynlink] {} @ 0x{:x} = 0x{:x}", sym_name, rela.r_offset, value);
                } else {
                    log::warn!("[dynlink] Unresolved symbol: {}", sym_name);
                }
            }
        }
        R_X86_64_64 => {
            // S + A
            if lib.symtab != 0 && r_sym > 0 {
                let sym = unsafe {
                    *((lib.symtab + (r_sym * core::mem::size_of::<Elf64Sym>()) as u64) as *const Elf64Sym)
                };
                let value = (lib.base_addr + sym.st_value).wrapping_add(rela.r_addend as u64);
                unsafe { *addr = value; }
            }
        }
        R_X86_64_NONE => {}
        _ => {
            log::warn!("[dynlink] Unknown relocation type: {}", r_type);
        }
    }
}

fn get_string(strtab: u64, offset: usize) -> String {
    let ptr = (strtab + offset as u64) as *const u8;
    let mut len = 0;
    
    unsafe {
        while *ptr.add(len) != 0 && len < 256 {
            len += 1;
        }
        
        let slice = core::slice::from_raw_parts(ptr, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

/// Call library init functions
pub fn call_init(lib: &LoadedLibrary) {
    if lib.init != 0 {
        log::info!("[dynlink] Calling init at 0x{:x}", lib.init);
        let init_fn: extern "C" fn() = unsafe { core::mem::transmute(lib.base_addr + lib.init) };
        init_fn();
    }
}
