use crate::interrupt::Interrupt;
use crate::memory::Memory;
use log::debug;
use std::cell::RefCell;
use std::rc::Rc;

pub const UART1_BASE: u16 = 0xD480;
pub const UART2_BASE: u16 = 0xD4A0;

/// Control register
const UART_CNTL: u16 = 0;
/// Command register
const UART_CMND: u16 = 1;
/// Status register
const UART_STAT: u16 = 2;
/// Receive ring buffer head register
const UART_RXHD: u16 = 4;
/// Receive ring buffer tail register
const UART_RXTL: u16 = 5;
/// Transmit ring buffer head register
const UART_TXHD: u16 = 6;
/// Transmit ring buffer tail register
const UART_TXTL: u16 = 7;
/// Ring buffer size
const UART_BUFFER_SIZE: u16 = 8;
/// Receive ring buffer (8 bytes)
const UART_RXBF: u16 = 8;
/// Transmit ring buffer (8 bytes)
const UART_TXBF: u16 = UART_RXBF + UART_BUFFER_SIZE;
/// End location
pub const UART_END: u16 = UART_TXBF + UART_BUFFER_SIZE;

#[derive(Debug, Clone)]
pub struct Uart {
    control: u8,
    command: u8,
    status: u8,
    receive_buffer: Rc<RefCell<RingBuf>>,
    transmit_buffer: Rc<RefCell<RingBuf>>,
    source: UartSource,
}

impl Uart {
    pub fn new(source: UartSource) -> Self {
        Self {
            control: 0,
            command: 0,
            status: 0,
            receive_buffer: Default::default(),
            transmit_buffer: Default::default(),
            source,
        }
    }

    pub const fn is_enabled(&self) -> bool {
        self.command & 0x1 != 0
    }

    pub fn update_state(&mut self) {
        // set enable/disable status bit
        if self.command & 0x1 != 0 {
            // discard all errors and transmit/receive status
            self.status = 0x40;
        } else {
            self.status = 0x0;
            self.receive_buffer.borrow_mut().set_head(0);
            self.transmit_buffer.borrow_mut().set_tail(0);
        }
    }

    pub const fn get_receive_buffer(&self) -> &Rc<RefCell<RingBuf>> {
        &self.receive_buffer
    }

    pub const fn get_transmit_buffer(&self) -> &Rc<RefCell<RingBuf>> {
        &self.transmit_buffer
    }
}

impl Memory for Uart {
    fn read_u8(&mut self, address: u16) -> u8 {
        match address {
            UART_CNTL => self.control,
            UART_CMND => self.command,
            UART_STAT => self.status,
            UART_RXHD => self.receive_buffer.borrow().head(),
            UART_RXTL => self.receive_buffer.borrow().tail(),
            UART_TXHD => self.transmit_buffer.borrow().head(),
            UART_TXTL => self.transmit_buffer.borrow().tail(),
            UART_RXBF..UART_TXBF => self
                .receive_buffer
                .borrow()
                .get((address - UART_RXBF) as u8),
            UART_TXBF..UART_END => self
                .transmit_buffer
                .borrow()
                .get((address - UART_TXBF) as u8),
            _ => 0,
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        match address {
            UART_CNTL => self.control = value,
            UART_CMND => {
                self.command = value;
            }
            UART_STAT => {
                // no-op
            }
            UART_RXHD => self.receive_buffer.borrow_mut().set_head(value),
            UART_RXTL => self.receive_buffer.borrow_mut().set_tail(value),
            UART_TXHD => self.transmit_buffer.borrow_mut().set_head(value),
            UART_TXTL => self.transmit_buffer.borrow_mut().set_tail(value),
            UART_RXBF..UART_TXBF => self
                .receive_buffer
                .borrow_mut()
                .set((address - UART_RXBF) as u8, value),
            UART_TXBF..UART_END => self
                .transmit_buffer
                .borrow_mut()
                .set((address - UART_TXBF) as u8, value),
            _ => {}
        }
    }

    fn update(&mut self, _cycle: usize) -> Interrupt {
        // TODO: this is kinda hacky
        self.update_state();
        if self.is_enabled() {
            // transmit
            {
                let mut tx = self.transmit_buffer.borrow_mut();
                while let Some(c) = tx.pop() {
                    // discard
                    debug!("UART tx: {:?} ({c})", c as char);
                }
            }

            // receive
            {
                let mut rx = self.receive_buffer.borrow_mut();
                while !rx.is_full() {
                    if let Some(value) = self.source.read() {
                        rx.push(value);
                        debug!(
                            "UART rx: push byte {:?} ({value}), remaining {}/{}",
                            value as char,
                            self.source.pos(),
                            self.source.len(),
                        )
                    } else {
                        break;
                    }
                }
            }
        }

        Interrupt::none()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RingBuf {
    buf: [u8; UART_BUFFER_SIZE as usize],
    head: u8,
    tail: u8,
}

impl RingBuf {
    pub const fn new() -> Self {
        Self {
            buf: [0; UART_BUFFER_SIZE as usize],
            head: 0,
            tail: 0,
        }
    }

    pub const fn capacity(&self) -> u8 {
        const _: () = assert!(0 < UART_BUFFER_SIZE && UART_BUFFER_SIZE <= u8::MAX as u16);

        UART_BUFFER_SIZE as u8
    }

    pub const fn len(&self) -> u8 {
        self.head.wrapping_sub(self.tail) % self.capacity()
    }

    pub const fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub const fn is_full(&self) -> bool {
        self.head.wrapping_add(1) % self.capacity() == self.tail
    }

    pub const fn head(&self) -> u8 {
        self.head
    }

    pub const fn set_head(&mut self, head: u8) {
        self.head = head % self.capacity();
    }

    pub const fn tail(&self) -> u8 {
        self.tail
    }

    pub const fn set_tail(&mut self, tail: u8) {
        self.tail = tail % self.capacity();
    }

    pub const fn push(&mut self, value: u8) -> bool {
        if self.is_full() {
            return false;
        }

        self.buf[self.head as usize] = value;
        self.head = self.head.wrapping_add(1) % self.capacity();
        true
    }

    pub const fn pop(&mut self) -> Option<u8> {
        if self.is_empty() {
            return None;
        }

        let value = self.buf[self.tail as usize];
        self.tail = self.tail.wrapping_add(1) % self.capacity();
        Some(value)
    }

    pub const fn get(&self, index: u8) -> u8 {
        self.buf[(index % self.capacity()) as usize]
    }

    pub const fn set(&mut self, index: u8, value: u8) {
        self.buf[(index % self.capacity()) as usize] = value;
    }
}

impl Default for RingBuf {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct UartSource {
    source: Vec<u8>,
    pos: usize,
}

impl UartSource {
    pub const fn empty() -> Self {
        Self {
            source: vec![],
            pos: 0,
        }
    }

    pub fn new(source: impl Into<Vec<u8>>) -> Self {
        Self {
            source: source.into(),
            pos: 0,
        }
    }

    pub const fn pos(&self) -> usize {
        self.pos
    }

    pub const fn len(&self) -> usize {
        self.source.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    pub fn has_next(&self) -> bool {
        self.pos < self.source.len()
    }

    pub fn read(&mut self) -> Option<u8> {
        if self.has_next() {
            let value = self.source[self.pos];
            self.pos += 1;
            Some(value)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_to_capacity() {
        let mut buf = RingBuf::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.push(1));
        assert_eq!(buf.len(), 1);
        assert!(buf.push(2));
        assert_eq!(buf.len(), 2);
        assert!(buf.push(3));
        assert_eq!(buf.len(), 3);
        assert!(buf.push(4));
        assert_eq!(buf.len(), 4);
        assert!(buf.push(5));
        assert_eq!(buf.len(), 5);
        assert!(buf.push(6));
        assert_eq!(buf.len(), 6);
        assert!(buf.push(7));
        assert_eq!(buf.len(), 7);
        assert!(!buf.push(8));
        assert_eq!(buf.len(), 7);
    }

    #[test]
    fn test_pop_empty() {
        let mut buf = RingBuf::new();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.pop(), None);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_pop_one() {
        let mut buf = RingBuf::new();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.pop(), None);
        assert_eq!(buf.len(), 0);
        assert!(buf.push(1));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.pop(), Some(1));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_pop_from_capacity() {
        let mut buf = RingBuf::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.push(1));
        assert!(buf.push(2));
        assert!(buf.push(3));
        assert!(buf.push(4));
        assert!(buf.push(5));
        assert!(buf.push(6));
        assert!(buf.push(7));
        assert!(!buf.push(8));
        assert_eq!(buf.len(), 7);
        assert_eq!(buf.pop(), Some(1));
        assert_eq!(buf.pop(), Some(2));
        assert_eq!(buf.pop(), Some(3));
        assert_eq!(buf.pop(), Some(4));
        assert_eq!(buf.pop(), Some(5));
        assert_eq!(buf.pop(), Some(6));
        assert_eq!(buf.pop(), Some(7));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_push_pop_one() {
        let mut buf = RingBuf::new();
        for i in 1..16 {
            assert_eq!(buf.len(), 0);
            assert!(buf.push(i));
            assert_eq!(buf.len(), 1);
            assert_eq!(buf.pop(), Some(i));
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_push_pop_two() {
        let mut buf = RingBuf::new();
        for i in 1..16 {
            assert_eq!(buf.len(), 0);
            assert!(buf.push(i));
            assert!(buf.push(i + 1));
            assert_eq!(buf.len(), 2);
            assert_eq!(buf.pop(), Some(i));
            assert_eq!(buf.pop(), Some(i + 1));
        }
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_status() {
        let mut buf = RingBuf::new();
        assert!(buf.is_empty());
        for i in 1..=7 {
            assert!(!buf.is_full());
            assert!(buf.push(i));
            assert!(!buf.is_empty());
        }
        assert!(buf.is_full());
    }
}
