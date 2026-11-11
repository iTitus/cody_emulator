use crate::memory::{Device, Memory};
use std::io::Write;
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

    /// Create memory with `data` placed at 0, discarding all overhang.
    pub fn from_bytes(size: u16, data: &[u8]) -> Self {
        Self::from_bytes_at(size, data, 0)
    }

    /// Create max size memory with `data` placed at 0, discarding all overhang.
    pub fn from_bytes_max_size(data: &[u8]) -> Self {
        Self::from_bytes_at_max_size(data, 0)
    }

    /// Create memory with `data` placed at `load_address`, discarding all overhang.
    pub fn from_bytes_at(size: u16, data: &[u8], load_address: u16) -> Self {
        let mut memory = Self::new(size);
        memory.force_write_all(load_address, data);
        memory
    }

    /// Create max size memory with `data` placed at `load_address`, discarding all overhang.
    pub fn from_bytes_at_max_size(data: &[u8], load_address: u16) -> Self {
        let mut memory = Self::new_max_size();
        memory.force_write_all(load_address, data);
        memory
    }

    pub const fn last(&self) -> u16 {
        (self.memory.len() - 1) as u16
    }

    pub fn force_write_u8(&mut self, address: u16, value: u8) {
        self.force_write_all(address, &[value]);
    }

    pub fn force_write_u16(&mut self, address: u16, value: u16) {
        self.force_write_all(address, &value.to_le_bytes());
    }

    pub fn force_write_all(&mut self, address: u16, data: &[u8]) {
        let remaining = self.memory.len().saturating_sub(address as usize);
        let to_copy = data.len().min(remaining);
        if to_copy > 0 {
            (&mut self.memory[address as usize..])
                .write_all(&data[..to_copy])
                .unwrap();
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
