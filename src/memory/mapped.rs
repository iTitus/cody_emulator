use crate::interrupt::Interrupt;
use crate::memory::{Device, Memory};

#[derive(Default)]
pub struct MappedMemory {
    memories: Vec<(u16, u16, Box<dyn Memory>)>,
    devices: Vec<Box<dyn Device>>,
}

impl MappedMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_memory(&mut self, first: u16, last: u16, memory: impl Memory + 'static) {
        self.memories.push((first, last, Box::new(memory)));
    }

    pub fn add_device(&mut self, device: impl Device + 'static) {
        self.devices.push(Box::new(device));
    }
}

impl Device for MappedMemory {
    fn update(&mut self, cycle: usize) -> Interrupt {
        let mut interrupt = Interrupt::none();
        for (_, _, memory) in &mut self.memories {
            interrupt = interrupt.or(memory.update(cycle));
        }
        for device in &mut self.devices {
            interrupt = interrupt.or(device.update(cycle));
        }
        interrupt
    }
}

impl Memory for MappedMemory {
    fn read_u8(&mut self, address: u16) -> u8 {
        for (first, last, memory) in self.memories.iter_mut().rev() {
            if (*first..=*last).contains(&address) {
                return memory.read_u8(address - *first);
            }
        }
        0 // fallback
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        for (first, last, memory) in self.memories.iter_mut().rev() {
            if (*first..=*last).contains(&address) {
                memory.write_u8(address - *first, value);
                return;
            }
        }
    }

    fn force_read_u8(&mut self, address: u16) -> u8 {
        for (first, last, memory) in self.memories.iter_mut().rev() {
            if (*first..=*last).contains(&address) {
                return memory.force_read_u8(address - *first);
            }
        }
        0 // fallback
    }

    fn force_write_u8(&mut self, address: u16, value: u8) {
        for (first, last, memory) in self.memories.iter_mut().rev() {
            if (*first..=*last).contains(&address) {
                memory.force_write_u8(address - *first, value);
                return;
            }
        }
    }
}
