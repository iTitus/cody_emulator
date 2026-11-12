use crate::memory::{Device, Memory};
use std::marker::PhantomData;

pub struct Contiguous<M = Ram> {
    pub memory: Box<[u8]>,
    _phantom: PhantomData<M>,
}

pub trait MemoryMode {
    const READABLE: bool;
    const WRITABLE: bool;
}

pub struct Ram;
pub struct Rom;

impl MemoryMode for Ram {
    const READABLE: bool = true;
    const WRITABLE: bool = true;
}

impl MemoryMode for Rom {
    const READABLE: bool = true;
    const WRITABLE: bool = false;
}

impl Contiguous<Ram> {
    pub fn new_ram(size: u16) -> Self {
        Self::new(size)
    }

    pub fn new_max_size_ram() -> Self {
        Self::new_max_size()
    }
}

impl Contiguous<Rom> {
    pub fn new_rom(size: u16) -> Self {
        Self::new(size)
    }

    pub fn new_max_size_rom() -> Self {
        Self::new_max_size()
    }
}

impl<M: MemoryMode> Contiguous<M> {
    pub fn new(size: u16) -> Self {
        Self {
            memory: vec![0; size as usize].into_boxed_slice(),
            _phantom: PhantomData,
        }
    }

    pub fn new_max_size() -> Self {
        Self {
            memory: vec![0; u16::MAX as usize + 1].into_boxed_slice(),
            _phantom: PhantomData,
        }
    }
}

impl<M: MemoryMode> Device for Contiguous<M> {}

impl<M: MemoryMode> Memory for Contiguous<M> {
    fn read_u8(&mut self, address: u16) -> u8 {
        if M::READABLE {
            self.force_read_u8(address)
        } else {
            0
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        if M::WRITABLE {
            self.force_write_u8(address, value);
        }
    }

    fn force_read_u8(&mut self, address: u16) -> u8 {
        if (address as usize) < self.memory.len() {
            self.memory[address as usize]
        } else {
            0
        }
    }

    fn force_write_u8(&mut self, address: u16, value: u8) {
        if (address as usize) < self.memory.len() {
            self.memory[address as usize] = value;
        }
    }
}
