use crate::opcode::Instruction;

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

impl Memory for [u8; 0x1000] {
    fn read_u8(&self, address: u16) -> u8 {
        self[address as usize]
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self[address as usize] = value;
    }
}

pub trait MemorySegment {
    fn read_u8(&self, address: u16) -> u8;
    fn write_u8(&mut self, address: u16, value: u8);
    fn size(&self) -> u16;
}

pub struct Program {
    ram: [u8; 0x200],
    rom: Vec<u8>,
    vector: [u8; 6],
}

impl Program {
    pub fn from_bytes(rom: impl Into<Vec<u8>>) -> Self {
        let mut rom = rom.into();
        rom.truncate(0xFFFF - 0x200 - 6);
        let mut rom = Self {
            ram: [0; 0x200],
            rom,
            vector: [0; 6],
        };
        rom.write_u16(0xFFFC, 0x0200);
        rom
    }

    pub fn from_instructions(instructions: &[Instruction]) -> Self {
        let mut rom = Self::from_bytes([]);
        for instruction in instructions {
            instruction
                .write(&mut rom.rom)
                .expect("invalid instruction");
        }
        rom
    }
}

impl Memory for Program {
    fn read_u8(&self, address: u16) -> u8 {
        if address < 0x0200 {
            self.ram[address as usize]
        } else if address >= 0xFFFA {
            self.vector[(address - 0xFFFA) as usize]
        } else {
            self.rom
                .get((address - 0x0200) as usize)
                .copied()
                .unwrap_or(0)
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        if address < 0x0200 {
            self.ram[address as usize] = value;
        } else if address >= 0xFFFA {
            self.vector[(address - 0xFFFA) as usize] = value;
        } else {
            unimplemented!();
        }
    }
}
