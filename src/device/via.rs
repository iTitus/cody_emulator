use crate::interrupt::Interrupt;
use crate::memory::Memory;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::cell::RefCell;
use std::rc::Rc;
use strum::{EnumCount, IntoStaticStr};

pub const VIA_IORB: usize = 0x0;
pub const VIA_IORA: usize = 0x1;
pub const VIA_DDRB: usize = 0x2;
pub const VIA_DDRA: usize = 0x3;
pub const VIA_T1CL: usize = 0x4;
pub const VIA_T1CH: usize = 0x5;
pub const VIA_T1LL: usize = 0x6;
pub const VIA_T1LH: usize = 0x7;
pub const VIA_T2CL: usize = 0x8;
pub const VIA_T2CH: usize = 0x9;
pub const VIA_SR: usize = 0xA;
pub const VIA_ACR: usize = 0xB;
pub const VIA_PCR: usize = 0xC;
pub const VIA_IFR: usize = 0xD;
pub const VIA_IER: usize = 0xE;
pub const VIA_IORA_NO_HANDSHAKE: usize = 0xF;

#[derive(Debug, Clone, Default)]
pub struct Via {
    registers: [u8; 16],
    key_state: Rc<RefCell<KeyState>>,
}

impl Via {
    fn read_iora(&mut self) -> u8 {
        let ddr = self.registers[VIA_DDRA];
        let ior = self.registers[VIA_IORA];
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
}

impl Memory for Via {
    fn read_u8(&mut self, address: u16) -> u8 {
        match address {
            0x1 => self.read_iora(),
            0x0..=0xF => self.registers[address as usize],
            _ => 0,
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        match address {
            0x0..=0xF => {
                self.registers[address as usize] = value;
            }
            _ => {}
        }
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        // TODO: properly implement timers and interrupts
        let t1c = u16::from_le_bytes([self.registers[VIA_T1CL], self.registers[VIA_T1CH]]);
        let acr = self.registers[VIA_ACR];
        let ier = self.registers[VIA_IER];
        if acr == 0x40 && ier == 0xC0 && cycle.is_multiple_of(t1c as usize) {
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
