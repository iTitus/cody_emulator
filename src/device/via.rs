use crate::interrupt::Interrupt;
use crate::memory::Memory;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::cell::RefCell;
use std::rc::Rc;
use strum::{EnumCount, IntoStaticStr};

pub const VIA_IORB: u16 = 0x0;
pub const VIA_IORA: u16 = 0x1;
pub const VIA_DDRB: u16 = 0x2;
pub const VIA_DDRA: u16 = 0x3;
pub const VIA_T1CL: u16 = 0x4;
pub const VIA_T1CH: u16 = 0x5;
pub const VIA_T1LL: u16 = 0x6;
pub const VIA_T1LH: u16 = 0x7;
pub const VIA_T2CL: u16 = 0x8;
pub const VIA_T2CH: u16 = 0x9;
pub const VIA_SR: u16 = 0xA;
pub const VIA_ACR: u16 = 0xB;
pub const VIA_PCR: u16 = 0xC;
pub const VIA_IFR: u16 = 0xD;
pub const VIA_IER: u16 = 0xE;
pub const VIA_IORA_NO_HANDSHAKE: u16 = 0xF;

#[derive(Debug, Clone, Default)]
pub struct Via {
    registers: [u8; 16],
    key_state: Rc<RefCell<KeyState>>,
    last_update: usize,
    t1_latch_lo: u8,
    t1_latch_hi: u8,
    t1_counter: u16,
    t1_enabled: bool,
    t2_latch_lo: u8,
    t2_latch_hi: u8,
    t2_counter: u16,
    t2_enabled: bool,
    ifr: u8,
    ier: u8,
}

impl Via {
    fn read_iora(&mut self) -> u8 {
        let ddr = self.registers[VIA_DDRA as usize];
        let ior = self.registers[VIA_IORA as usize];
        // TODO: only works for cody right now
        assert_eq!(
            ddr, 0x7,
            "when reading IORA only DDRA = 0x7 is supported, but was {ddr:#x}"
        );
        let output = ior & ddr;
        self.key_state.borrow().state[output as usize] | output
    }

    pub fn get_key_state(&self) -> &Rc<RefCell<KeyState>> {
        &self.key_state
    }

    fn set_ifr(&mut self, ifr: u8) {
        let mut ifr = ifr & 0x7F;
        if (ifr & self.ier) != 0 {
            ifr |= 0x80;
        }
        self.ifr = ifr;
    }

    fn set_ier(&mut self, ier: u8) {
        if (ier & 0x80) != 0 {
            self.ier |= ier;
        } else {
            self.ier &= !ier;
        }

        // update ifr bit 7
        self.set_ifr(self.ifr);
    }
}

impl Memory for Via {
    fn read_u8(&mut self, address: u16) -> u8 {
        match address {
            VIA_IORA => self.read_iora(),
            VIA_T1CL => {
                self.set_ifr(self.ifr & !0x40);
                (self.t1_counter & 0xFF) as u8
            }
            VIA_T1CH => (self.t1_counter >> 8) as u8,
            VIA_T1LL => self.t1_latch_lo,
            VIA_T1LH => self.t1_latch_hi,
            VIA_T2CL => {
                self.set_ifr(self.ifr & !0x20);
                (self.t2_counter & 0xFF) as u8
            }
            VIA_T2CH => (self.t2_counter >> 8) as u8,
            VIA_IFR => self.ifr,
            VIA_IER => self.ier | 0x80,
            0x0..=0xF => self.registers[address as usize],
            _ => 0,
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        match address {
            VIA_T1CL => self.t1_latch_lo = value,
            VIA_T1CH => {
                self.t1_latch_hi = value;
                self.set_ifr(self.ifr & !0x40);
                self.t1_counter = self.t1_latch_lo as u16 | (self.t1_latch_hi as u16) << 8;
                self.t1_enabled = true;
            }
            VIA_T1LL => self.t1_latch_lo = value,
            VIA_T1LH => {
                self.t1_latch_hi = value;
                self.set_ifr(self.ifr & !0x40);
            }
            VIA_T2CL => self.t2_latch_lo = value,
            VIA_T2CH => {
                self.t2_latch_hi = value;
                self.set_ifr(self.ifr & !0x20);
                self.t2_counter = self.t2_latch_lo as u16 | (self.t2_latch_hi as u16) << 8;
                self.t2_enabled = true;
            }
            VIA_IFR => self.set_ifr(value),
            VIA_IER => self.set_ier(value),
            0x0..=0xF => {
                self.registers[address as usize] = value;
            }
            _ => {}
        }
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        let cycles_elapsed = cycle.wrapping_sub(self.last_update);
        self.last_update = cycle;

        let acr = self.registers[VIA_ACR as usize];

        for _ in 0..cycles_elapsed {
            self.t1_counter = self.t1_counter.wrapping_sub(1);
            if self.t1_counter == 0 {
                if self.t1_enabled {
                    self.set_ifr(self.ifr | 0x40);

                    // if not in continuous mode we stop the interrupt trigger
                    if (acr & 0x40) == 0 {
                        self.t1_enabled = false;
                    }
                }

                // reset counter to latched value
                self.t1_counter = self.t1_latch_lo as u16 | (self.t1_latch_hi as u16) << 8;

                if (acr & 0x80) != 0 {
                    // TODO: flip PB7
                }
            }

            if (acr & 0x20) != 0 {
                // TODO: count PB6 pulses
            } else {
                self.t2_counter = self.t2_counter.wrapping_sub(1);
            }

            if self.t2_counter == 0 && self.t2_enabled {
                self.set_ifr(self.ifr | 0x20);
                self.t2_enabled = false;
            }
        }

        if (self.ifr & 0x80) != 0 {
            Interrupt::irq()
        } else {
            Interrupt::none()
        }
    }
}

#[repr(u8)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoPrimitive,
    TryFromPrimitive,
    EnumCount,
    IntoStaticStr,
)]
pub enum CodyKeyCode {
    KeyQ = 0,
    KeyE = 1,
    KeyT = 2,
    KeyU = 3,
    KeyO = 4,
    KeyA = 5,
    KeyD = 6,
    KeyG = 7,
    KeyJ = 8,
    KeyL = 9,
    Cody = 10,
    KeyX = 11,
    KeyV = 12,
    KeyN = 13,
    Meta = 14,
    KeyZ = 15,
    KeyC = 16,
    KeyB = 17,
    KeyM = 18,
    Enter = 19,
    KeyS = 20,
    KeyF = 21,
    KeyH = 22,
    KeyK = 23,
    Space = 24,
    KeyW = 25,
    KeyR = 26,
    KeyY = 27,
    KeyI = 28,
    KeyP = 29,
    Joystick1Up = 30,
    Joystick1Down = 31,
    Joystick1Left = 32,
    Joystick1Right = 33,
    Joystick1Fire = 34,
    Joystick2Up = 35,
    Joystick2Down = 36,
    Joystick2Left = 37,
    Joystick2Right = 38,
    Joystick2Fire = 39,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct KeyState {
    state: [u8; 8],
}

impl KeyState {
    pub fn set_pressed(&mut self, code: CodyKeyCode, pressed: bool) {
        let code = code as u8;
        let bit = (code % 5) + 3;
        let index = code / 5;
        let mask = 1 << bit;
        if pressed {
            self.state[index as usize] &= !mask;
        } else {
            self.state[index as usize] |= mask;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum CodyModifier {
    Cody,
    Meta,
}
