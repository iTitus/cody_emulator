use crate::device::MemoryDevice;

#[derive(Debug, Copy, Clone)]
pub struct Via {
    /// 0: VIA_IORB
    ///
    /// 1: VIA_IORA
    ///
    /// 2: VIA_DDRB
    ///
    /// 3: VIA_DDRA
    ///
    /// 4: VIA_T1CL
    ///
    /// 5: VIA_T1CH
    ///
    /// 6: VIA_T1LL
    ///
    /// 7: VIA_T1LH
    ///
    /// 8: VIA_T2CL
    ///
    /// 9: VIA_T2CH
    ///
    /// A: VIA_SR
    ///
    /// B: VIA_ACR
    ///
    /// C: VIA_PCR
    ///
    /// D: VIA_IFR
    ///
    /// E: VIA_IER
    ///
    /// F: VIA_IORA (no handshake)
    registers: [u8; 16],
    key_state: [u8; 8],
}

impl Via {
    fn read_iora(&mut self) -> u8 {
        let ddr = self.registers[3];
        let ior = self.registers[1];
        // TODO: only works for cody right now
        assert_eq!(
            ddr, 0x7,
            "when reading IORA only DDRA = 0x7 is supported, but was {ddr:#x}"
        );
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
