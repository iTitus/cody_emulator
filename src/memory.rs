use crate::cpu;
use crate::device::MemoryDevice;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub trait Memory {
    fn read_u8(&mut self, address: u16) -> u8;

    fn read_u16(&mut self, address: u16) -> u16 {
        let l = self.read_u8(address);
        let h = self.read_u8(address.wrapping_add(1));
        u16::from_le_bytes([l, h])
    }

    fn write_u8(&mut self, address: u16, value: u8);

    fn write_u16(&mut self, address: u16, value: u16) {
        let [l, h] = value.to_le_bytes();
        self.write_u8(address, l);
        self.write_u8(address.wrapping_add(1), h);
    }

    fn on_cycle(&mut self) {}

    fn on_instruction_finished(&mut self) {}
}

impl<M: Memory> Memory for Arc<Mutex<M>> {
    fn read_u8(&mut self, address: u16) -> u8 {
        self.lock().unwrap().read_u8(address)
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.lock().unwrap().write_u8(address, value);
    }
}

pub struct Contiguous(pub [u8; 0x10000]);

impl Default for Contiguous {
    fn default() -> Self {
        // TODO: does this stop stack overflows?
        Self(vec![0; 0x10000].try_into().unwrap())
    }
}

impl Memory for Contiguous {
    fn read_u8(&mut self, address: u16) -> u8 {
        self.0[address as usize]
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.0[address as usize] = value;
    }
}

impl Contiguous {
    /// Create memory with `data` placed at 0.
    pub fn from_bytes(data: &[u8]) -> Self {
        Self::from_bytes_at(data, 0)
    }

    /// Create memory with `data` placed at `load_address`.
    pub fn from_bytes_at(data: &[u8], load_address: u16) -> Self {
        let mut memory = Self::default();
        let remaining = memory.0.len() - load_address as usize;
        let to_copy = data.len().min(remaining);
        (&mut memory.0[load_address as usize..])
            .write_all(&data[..to_copy])
            .unwrap();
        memory
    }
}

pub struct Sparse {
    pub zeropage: [u8; 0x100],
    pub stack: [u8; 0x100],
    pub last_page: [u8; 0x100],
    pub memory: Vec<u8>,
}

impl Default for Sparse {
    fn default() -> Self {
        Self {
            zeropage: [0; 0x100],
            stack: [0; 0x100],
            last_page: [0; 0x100],
            memory: vec![],
        }
    }
}

impl Memory for Sparse {
    fn read_u8(&mut self, address: u16) -> u8 {
        match address {
            0x0000..0x0100 => self.zeropage[(address & 0xFF) as usize],
            0x0100..0x0200 => self.stack[(address & 0xFF) as usize],
            0xFF00.. => self.last_page[(address & 0xFF) as usize],
            _ => self
                .memory
                .get((address - 0x0200) as usize)
                .copied()
                .unwrap_or(0),
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        match address {
            0x0000..0x0100 => self.zeropage[(address & 0xFF) as usize] = value,
            0x0100..0x0200 => self.stack[(address & 0xFF) as usize] = value,
            0xFF00.. => self.last_page[(address & 0xFF) as usize] = value,
            _ => {
                *self
                    .memory
                    .get_mut((address - 0x0200) as usize)
                    .expect("out of bounds write in sparse memory") = value;
            }
        }
    }
}

impl Sparse {
    const MAX_LEN: usize = 0x10000 - 0x0300;

    /// Create memory with data placed at 0x0200 and the reset vector configured accordingly.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut m = Self {
            memory: data[..data.len().min(Self::MAX_LEN)].into(),
            ..Default::default()
        };
        m.write_u16(cpu::RESET_VECTOR, 0x0200);
        m
    }
}

#[derive(Default)]
pub struct MappedMemory<M> {
    base: M,
    devices: Vec<Box<dyn MemoryDevice + Send>>,
}

impl<M: Memory> MappedMemory<M> {
    pub fn from_memory(memory: M) -> Self {
        Self {
            base: memory,
            devices: vec![],
        }
    }

    pub fn add_device<MH: MemoryDevice + Send + 'static>(&mut self, handler: MH) {
        self.devices.push(Box::new(handler));
    }
}

impl<M: Memory> Memory for MappedMemory<M> {
    fn read_u8(&mut self, address: u16) -> u8 {
        for device in &mut self.devices {
            if let Some(value) = device.read(address) {
                return value;
            }
        }
        self.base.read_u8(address)
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        for device in &mut self.devices {
            if device.write(address, value).is_some() {
                return;
            }
        }
        self.base.write_u8(address, value);
    }

    fn on_cycle(&mut self) {
        self.devices.iter_mut().for_each(|d| d.on_cycle());
    }

    fn on_instruction_finished(&mut self) {
        self.devices
            .iter_mut()
            .for_each(|d| d.on_instruction_finished());
    }
}
