use crate::interrupt::Interrupt;
use crate::memory::Memory;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MemoryAccessType {
    Read,
    Write,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MemoryAccess {
    pub access_type: MemoryAccessType,
    pub address: u16,
    pub value: u8,
}

impl MemoryAccess {
    pub const fn read(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Read,
            address,
            value,
        }
    }

    pub const fn write(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Write,
            address,
            value,
        }
    }
}

#[derive(Debug)]
pub struct LoggingMemory<M> {
    inner: M,
    log: Vec<MemoryAccess>,
}

impl<M: Memory> LoggingMemory<M> {
    pub const fn new(memory: M) -> Self {
        Self {
            inner: memory,
            log: vec![],
        }
    }

    pub fn log(&self) -> &[MemoryAccess] {
        &self.log
    }

    pub fn reset_log(&mut self) {
        self.log.clear();
    }
}

impl<M: Memory + Default> Default for LoggingMemory<M> {
    fn default() -> Self {
        Self {
            inner: M::default(),
            log: vec![],
        }
    }
}

impl<M: Memory> Memory for LoggingMemory<M> {
    fn read_u8(&mut self, address: u16) -> u8 {
        let value = self.inner.read_u8(address);
        self.log.push(MemoryAccess::read(address, value));
        value
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.inner.write_u8(address, value);
        self.log.push(MemoryAccess::write(address, value));
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        self.inner.update(cycle)
    }
}
