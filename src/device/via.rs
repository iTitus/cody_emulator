use crate::interrupt::Interrupt;
use crate::memory::Memory;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::cell::RefCell;
use std::rc::Rc;
use strum::{EnumCount, IntoStaticStr};

#[derive(Debug, Clone, Default)]
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
    key_state: Rc<RefCell<KeyState>>,
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

    fn update(&mut self, _cycle: usize) -> Interrupt {
        // TODO: implement timer
        Interrupt::none()
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
