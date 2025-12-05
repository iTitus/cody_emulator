use crate::interrupt::Interrupt;
use crate::memory::Memory;

#[derive(Default)]
pub struct MappedMemory {
    memories: Vec<(u16, u16, Box<dyn Memory>)>,
}

impl MappedMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_memory(&mut self, address: u16, size: u16, memory: impl Memory + 'static) {
        self.memories.push((address, size, Box::new(memory)));
    }

    pub fn add_device(&mut self, memory: impl Memory + 'static) {
        self.add_memory(0, 0, memory);
    }
}

impl Memory for MappedMemory {
    fn read_u8(&mut self, address: u16) -> u8 {
        for (start, size, memory) in self.memories.iter_mut().rev() {
            if *size == 0 {
                continue;
            }
            if (*start..=start.saturating_add(*size - 1)).contains(&address) {
                return memory.read_u8(address - *start);
            }
        }
        0 // fallback
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        for (start, size, memory) in self.memories.iter_mut().rev() {
            if *size == 0 {
                continue;
            }
            if (*start..=start.saturating_add(*size - 1)).contains(&address) {
                return memory.write_u8(address - *start, value);
            }
        }
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        let mut interrupt = Interrupt::none();
        for (_, _, memory) in &mut self.memories {
            interrupt = interrupt.or(memory.update(cycle));
        }
        interrupt
    }
}
