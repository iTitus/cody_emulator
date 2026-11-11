use crate::memory::{Device, Memory};

#[derive(Debug, Copy, Clone)]
pub struct ZeroMemory;

impl Device for ZeroMemory {}

impl Memory for ZeroMemory {
    fn read_u8(&mut self, _address: u16) -> u8 {
        0
    }

    fn write_u8(&mut self, _address: u16, _value: u8) {}
}
