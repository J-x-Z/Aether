# Aether

**Aether** is a hybrid kernel designed for bare-metal execution with POSIX and glibc compatibility.

## Features

- 🚀 **UEFI Native Boot** - Direct boot from UEFI firmware
- 🔧 **Hybrid Architecture** - Combines microkernel flexibility with monolithic performance
- 🐧 **POSIX Compatible** - Linux syscall ABI compatibility
- 📚 **glibc Support** - Run standard Linux applications
- 💻 **Multi-Architecture** - x86_64 and AArch64 support

## Architecture

```
┌─────────────────────────────────────────────┐
│              User Applications              │
├─────────────────────────────────────────────┤
│           POSIX Syscall Interface           │
├──────────┬──────────┬──────────┬────────────┤
│   mm/    │  sched/  │   fs/    │  drivers/  │
│ Memory   │Scheduler │Filesystem│  Drivers   │
├──────────┴──────────┴──────────┴────────────┤
│              Hardware Abstraction           │
├─────────────────────────────────────────────┤
│          UEFI / Bare Metal Hardware         │
└─────────────────────────────────────────────┘
```

## Building

```bash
# Build for x86_64 UEFI
cargo build --target x86_64-unknown-uefi

# Build for AArch64 (planned)
cargo build --target aarch64-unknown-uefi
```

## Running

```bash
# QEMU with OVMF
qemu-system-x86_64 \
  -bios /path/to/OVMF.fd \
  -drive format=raw,file=fat:rw:esp \
  -nographic
```

## Project Structure

```
Aether/
├── src/           # Kernel source
│   ├── arch/      # Architecture-specific (x86_64, aarch64)
│   ├── mm/        # Memory management
│   ├── sched/     # Process scheduler
│   ├── syscall/   # POSIX syscalls
│   ├── fs/        # Filesystem (VFS, ext2, FAT)
│   └── drivers/   # Device drivers
├── aether-core/   # Shared kernel abstractions
└── abi/           # Application Binary Interface
```

## Related Projects

- [AetherOS](https://github.com/J-x-Z/AetherOS) - Cross-platform software stack built on Aether

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.
