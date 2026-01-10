use spin::Mutex;
use alloc::collections::VecDeque;
use lazy_static::lazy_static;

lazy_static! {
    static ref STDIN_BUFFER: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::with_capacity(128));
}

pub fn push_char(c: char) {
    let mut buf = STDIN_BUFFER.lock();
    // Basic ASCII support for now
    if c.is_ascii() {
        buf.push_back(c as u8);
    }
}

pub fn pop_char() -> Option<u8> {
    let mut buf = STDIN_BUFFER.lock();
    buf.pop_front()
}

pub fn has_data() -> bool {
    let buf = STDIN_BUFFER.lock();
    !buf.is_empty()
}
