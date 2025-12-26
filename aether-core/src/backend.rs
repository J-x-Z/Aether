#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExitReason {
    Yield,      // Time slice expired or voluntary yield
    Io(u16),    // IO Access (Port)
    Mmio(u64),  // MMIO Access (Address)
    Halt,       // Guest Halted
    Unknown,    // Other exit reasons
}

pub trait Backend: Sync + Send {
    fn name(&self) -> &str;
    
    /// Run the vCPU corresponding to one time slice or until an exit event.
    fn step(&self) -> ExitReason;

    // Framebuffer access
    unsafe fn get_framebuffer(&self, width: usize, height: usize) -> &[u32];

    // Inject a key press into the Guest
    fn inject_key(&self, _c: char) {}
}
