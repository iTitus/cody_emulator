use crate::device::MemoryDevice;

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
const UART_END: u16 = UART_TXBF + UART_BUFFER_SIZE;

#[derive(Debug, Copy, Clone)]
pub struct Uart {
    base_address: u16,
    control: u8,
    command: u8,
    status: u8,
    pub receive_buffer: RingBuf,
    pub transmit_buffer: RingBuf,
}

impl Uart {
    pub const fn new(base_address: u16) -> Self {
        Self {
            base_address,
            control: 0,
            command: 0,
            status: 0,
            receive_buffer: RingBuf::new(),
            transmit_buffer: RingBuf::new(),
        }
    }

    pub const fn is_enabled(&self) -> bool {
        self.command & 0x1 != 0
    }

    pub const fn update_state(&mut self) {
        // set enable/disable status bit
        if self.command & 0x1 != 0 {
            // discard all errors and transmit/receive status
            self.status = 0x40;
        } else {
            self.status = 0;
            self.receive_buffer.set_head(0);
            self.transmit_buffer.set_tail(0);
        }
    }
}

impl MemoryDevice for Uart {
    fn read(&mut self, address: u16) -> Option<u8> {
        if !(self.base_address..self.base_address + UART_END).contains(&address) {
            return None;
        }
        let offset = address - self.base_address;
        Some(match offset {
            UART_CNTL => self.control,
            UART_CMND => self.command,
            UART_STAT => self.status,
            UART_RXHD => self.receive_buffer.head(),
            UART_RXTL => self.receive_buffer.tail(),
            UART_TXHD => self.transmit_buffer.head(),
            UART_TXTL => self.transmit_buffer.tail(),
            UART_RXBF..UART_TXBF => self.receive_buffer.get((offset - UART_RXBF) as u8),
            UART_TXBF..UART_END => self.transmit_buffer.get((offset - UART_TXBF) as u8),
            _ => 0,
        })
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        if !(self.base_address..self.base_address + UART_END).contains(&address) {
            return None;
        }
        let offset = address - self.base_address;
        match offset {
            UART_CNTL => self.control = value,
            UART_CMND => {
                self.command = value;
            }
            UART_STAT => {
                // no-op
            }
            UART_RXHD => self.receive_buffer.set_head(value),
            UART_RXTL => self.receive_buffer.set_tail(value),
            UART_TXHD => self.transmit_buffer.set_head(value),
            UART_TXTL => self.transmit_buffer.set_tail(value),
            UART_RXBF..UART_TXBF => self.receive_buffer.set((offset - UART_RXBF) as u8, value),
            UART_TXBF..UART_END => self.transmit_buffer.set((offset - UART_TXBF) as u8, value),
            _ => {}
        };
        Some(())
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
