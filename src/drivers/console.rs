use x86_64::instructions::port::Port;

// COM1
const COM1: u16 = 0x3F8;

pub fn init() {
    init_serial();
}

fn init_serial() {
    // Basic Serial Port Initialization for COM1
    unsafe {
        let mut port_ier = Port::<u8>::new(COM1 + 1); // Interrupt Enable
        let mut port_lcr = Port::<u8>::new(COM1 + 3); // Line Control
        let mut port_mcr = Port::<u8>::new(COM1 + 4); // Modem Control
        
        // Disable interrupts
        port_ier.write(0x00);
        
        // Enable DLAB (set baud rate divisor)
        port_lcr.write(0x80);
        
        // Set divisor to 3 (38400 baud) - low byte 3, high byte 0
        let mut port_dll = Port::<u8>::new(COM1);
        let mut port_dlh = Port::<u8>::new(COM1 + 1);
        port_dll.write(0x03);
        port_dlh.write(0x00);
        
        // 8 bits, no parity, one stop bit, clear DLAB
        port_lcr.write(0x03);
        
        // Enable FIFO, clear them, with 14-byte threshold
        let mut port_fcr = Port::<u8>::new(COM1 + 2);
        port_fcr.write(0xC7);
        
        // IRQs enabled, RTS/DSR set
        port_mcr.write(0x0B);
        
        // Enable 'Received Data Available' Interrupt
        port_ier.write(0x01);
    }
    log::info!("[Drivers] Serial COM1 Initialized (IRQ Enabled)");
}

pub fn read_serial() -> Option<u8> {
    unsafe {
        let mut port_lsr = Port::<u8>::new(COM1 + 5); // Line Status
        if port_lsr.read() & 1 == 0 {
            return None; // No data
        }
        let mut port_data = Port::<u8>::new(COM1);
        Some(port_data.read())
    }
}

pub fn is_data_ready() -> bool {
    unsafe {
        let mut port_lsr = Port::<u8>::new(COM1 + 5);
        port_lsr.read() & 1 != 0
    }
}

/// Write a byte to serial port (for stdout output)
pub fn write_serial(byte: u8) {
    unsafe {
        let mut port_lsr = Port::<u8>::new(COM1 + 5);
        // Wait for transmit buffer to be empty
        while port_lsr.read() & 0x20 == 0 {}
        let mut port_data = Port::<u8>::new(COM1);
        port_data.write(byte);
    }
}
