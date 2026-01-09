# Aether

**Aether** is a hybrid kernel designed for bare-metal execution with POSIX and glibc compatibility.

## 🎉 Current Milestone: BusyBox Shell

Aether now boots directly into a **BusyBox shell** (static musl build), making it a functional POSIX-compatible operating system!

### Features Implemented

- ✅ **30+ POSIX Syscalls** - read, write, open, close, fork, execve, mmap, brk, etc.
- ✅ **ELF Loader** - Full ELF64 parser with PT_LOAD segment handling
- ✅ **Dynamic Linker Support** - PT_INTERP detection, Auxiliary Vector (Auxv) setup
- ✅ **Virtual Filesystems** - /dev (null, zero, urandom, tty), /proc (self, meminfo, cpuinfo)
- ✅ **BusyBox 1.35** - Static musl binary embedded in kernel initrd

## Architecture

```
┌─────────────────────────────────────────────┐
│           BusyBox Shell (User Space)        │
├─────────────────────────────────────────────┤
│           POSIX Syscall Interface           │
├──────────┬──────────┬──────────┬────────────┤
│   mm/    │  sched/  │   fs/    │  drivers/  │
│ Memory   │Scheduler │VFS+DevFS │  Keyboard  │
├──────────┴──────────┴──────────┴────────────┤
│              Hardware Abstraction           │
├─────────────────────────────────────────────┤
│          UEFI / Bare Metal Hardware         │
└─────────────────────────────────────────────┘
```

## Building

```bash
# Install Rust nightly
rustup toolchain install nightly
rustup target add x86_64-unknown-uefi --toolchain nightly

# Build for x86_64 UEFI
cargo build --release --target x86_64-unknown-uefi
```

## Running on QEMU

### Windows

```powershell
# 1. Install QEMU: https://www.qemu.org/download/#windows
# 2. Download OVMF: https://www.kraxel.org/repos/jenkins/edk2/
#    Extract edk2.git-ovmf-x64-*.noarch.rpm and get OVMF_CODE.fd

# 3. Create ESP directory
mkdir esp\EFI\BOOT
copy target\x86_64-unknown-uefi\release\aether.efi esp\EFI\BOOT\BOOTX64.EFI

# 4. Run QEMU
qemu-system-x86_64.exe -m 512M -bios C:\path\to\OVMF_CODE.fd -drive format=raw,file=fat:rw:esp -serial mon:stdio
```

### Linux

```bash
./run-qemu-x86.sh
```

### macOS (Intel only)

```bash
brew install qemu
./run-qemu-x86.sh
```

> ⚠️ **Note**: Apple Silicon Macs cannot run x86 QEMU with UEFI due to TCG limitations.

## Project Structure

```
Aether/
├── src/
│   ├── arch/       # x86_64, aarch64 architecture code
│   ├── mm/         # Memory management, paging
│   ├── sched/      # Process scheduler, task management
│   ├── syscall/    # POSIX syscalls, ELF loader
│   ├── fs/         # VFS, RamFS, DevFS, ProcFS
│   └── drivers/    # Keyboard, serial drivers
├── init/
│   ├── init.bin    # Simple init binary
│   └── busybox.bin # BusyBox 1.35 (musl static)
└── aether-core/    # Shared kernel abstractions
```

## Roadmap

- [x] Phase 14: POSIX Syscall Coverage
- [x] Phase 15: ELF Dynamic Linker (kernel support)
- [x] Phase 16: Virtual Filesystems (/dev, /proc)
- [x] Phase 17: BusyBox Shell (current milestone)
- [ ] Phase 18: ext4 Filesystem
- [ ] Phase 19: Real x86 Hardware Boot

## Related Projects

- [AetherOS](https://github.com/J-x-Z/AetherOS) - Cross-platform software stack

## License

GNU General Public License v3.0 - see [LICENSE](LICENSE)
