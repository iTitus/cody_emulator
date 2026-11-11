use crate::interrupt::Interrupt;
use crate::memory::{Device, Memory};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MemoryAccessType {
    Read,
    Write,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MemoryAccess {
    pub access_type: MemoryAccessType,
    pub force: bool,
    pub address: u16,
    pub value: u8,
}

impl MemoryAccess {
    pub const fn read(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Read,
            address,
            value,
            force: false,
        }
    }

    pub const fn force_read(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Read,
            address,
            value,
            force: true,
        }
    }

    pub const fn write(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Write,
            address,
            value,
            force: false,
        }
    }

    pub const fn force_write(address: u16, value: u8) -> Self {
        Self {
            access_type: MemoryAccessType::Write,
            address,
            value,
            force: true,
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

impl<D: Device> Device for LoggingMemory<D> {
    fn update(&mut self, cycle: usize) -> Interrupt {
        self.inner.update(cycle)
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

    fn force_read_u8(&mut self, address: u16) -> u8 {
        let value = self.inner.force_read_u8(address);
        self.log.push(MemoryAccess::force_read(address, value));
        value
    }

    fn force_write_u8(&mut self, address: u16, value: u8) {
        self.inner.force_write_u8(address, value);
        self.log.push(MemoryAccess::force_write(address, value));
    }
}
