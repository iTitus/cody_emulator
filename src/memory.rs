use crate::opcode::{assemble, Instruction};

pub trait Memory {
    fn read_u8(&self, address: u16) -> u8;

    fn read_u16(&self, address: u16) -> u16 {
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
}

struct Contiguous([u8; 0x10000]);

impl Default for Contiguous {
    fn default() -> Self {
        Self([0; 0x10000])
    }
}

impl Memory for Contiguous {
    fn read_u8(&self, address: u16) -> u8 {
        self.0[address as usize]
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.0[address as usize] = value;
    }
}

struct Sparse {
    zeropage: [u8; 0x100],
    stack: [u8; 0x100],
    last_page: [u8; 0x100],
    memory: Vec<u8>,
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
    fn read_u8(&self, address: u16) -> u8 {
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

/// Create memory with instructions placed at load_address and the reset vector configured accordingly.
pub fn memory_from_instructions(
    instructions: &[Instruction],
    load_address: u16,
) -> impl Memory + use<> {
    let mut memory = Contiguous::default();
    assemble(instructions, &mut memory.0[load_address as usize..])
        .expect("error assembling instructions");
    memory.write_u16(0xFFFC, load_address);
    memory
}

/// Create memory with data placed at load_address.
pub fn memory_from_bytes(data: &[u8], load_address: u16) -> impl Memory + use<> {
    let mut memory = Contiguous::default();
    memory.0[load_address as usize..].copy_from_slice(data);
    memory
}

/// Create sparse memory with instructions placed at 0x0200 and the reset vector configured accordingly.
///
/// Note that sparse memory cannot be written to arbitrarily.
pub fn sparse_memory_from_instructions(instructions: &[Instruction]) -> impl Memory + use<> {
    const MAX_LEN: usize = 0x10000 - 0x0300;

    let mut memory = Sparse::default();
    assemble(instructions, &mut memory.memory).expect("error assembling instructions");
    assert!(memory.memory.len() < MAX_LEN);
    memory
        .memory
        .resize(memory.memory.capacity().min(MAX_LEN), 0); // allocator gave us more bytes, so use them
    memory.write_u16(0xFFFC, 0x0200);
    memory
}
