use crate::interrupt::Interrupt;
use crate::memory::Memory;
use std::io::Write;
use std::marker::PhantomData;

pub struct Contiguous<M = Ram> {
    pub memory: Box<[u8]>,
    _phantom: PhantomData<M>,
}

pub trait MemoryMode {
    fn is_writeable() -> bool;
}

pub struct Ram;
pub struct Rom;

impl MemoryMode for Ram {
    fn is_writeable() -> bool {
        true
    }
}

impl MemoryMode for Rom {
    fn is_writeable() -> bool {
        false
    }
}

impl Contiguous<Ram> {
    pub fn new_ram(size: usize) -> Self {
        Self::new(size)
    }
}

impl Contiguous<Rom> {
    pub fn new_rom(size: usize) -> Self {
        Self::new(size)
    }
}

impl<M: MemoryMode> Contiguous<M> {
    pub fn new(size: usize) -> Self {
        Self {
            memory: vec![0; size].into_boxed_slice(),
            _phantom: PhantomData,
        }
    }

    /// Create memory with `data` placed at 0, discarding all overhang.
    pub fn from_bytes(size: usize, data: &[u8]) -> Self {
        Self::from_bytes_at(size, data, 0)
    }

    /// Create memory with `data` placed at `load_address`, discarding all overhang.
    pub fn from_bytes_at(size: usize, data: &[u8], load_address: u16) -> Self {
        let mut memory = Self::new(size);
        memory.force_write_all(load_address, data);
        memory
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

impl<M: MemoryMode> Memory for Contiguous<M> {
    fn read_u8(&mut self, address: u16) -> u8 {
        self.memory[address as usize % self.memory.len()]
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        if M::is_writeable() {
            self.memory[address as usize % self.memory.len()] = value;
        }
    }

    fn update(&mut self, _cycle: usize) -> Interrupt {
        Interrupt::none()
    }
}
