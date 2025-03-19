use crate::device::MemoryDevice;

#[derive(Debug, Copy, Clone)]
pub struct Via {
    registers: [u8; 16],
    key_state: [u8; 8],
}

impl Via {
    fn read_iora(&mut self) -> u8 {
        let ddr = self.registers[3];
        let ior = self.registers[1];
        assert_eq!(ddr, 0x7); // TODO: only works for cody right now
        let output = ior & ddr;
        self.key_state[output as usize] | output
    }

    pub fn set_pressed(&mut self, code: u8, pressed: bool) {
        let bit = (code % 5) + 3;
        let index = code / 5;
        let mask = 1 << bit;
        if pressed {
            self.key_state[index as usize] &= !mask;
        } else {
            self.key_state[index as usize] |= mask;
        }
    }
}

impl Default for Via {
    fn default() -> Self {
        Self {
            registers: [0; 16],
            key_state: [0xF8; 8],
        }
    }
}

impl MemoryDevice for Via {
    fn read(&mut self, address: u16) -> Option<u8> {
        match address {
            0x9F01 => Some(self.read_iora()),
            0x9F00..=0x9F0F => Some(self.registers[(address - 0x9F00) as usize]),
            _ => None,
        }
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        match address {
            0x9F00..=0x9F0F => {
                self.registers[(address - 0x9F00) as usize] = value;
                Some(())
            }
            _ => None,
        }
    }
}
