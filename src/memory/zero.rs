use crate::interrupt::Interrupt;
use crate::memory::Memory;

#[derive(Debug, Copy, Clone)]
pub struct ZeroMemory;

impl Memory for ZeroMemory {
    fn read_u8(&mut self, _address: u16) -> u8 {
        0
    }

    fn write_u8(&mut self, _address: u16, _value: u8) {}

    fn update(&mut self, _cycle: usize) -> Interrupt {
        Interrupt::none()
    }
}
