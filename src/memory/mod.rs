use crate::interrupt::Interrupt;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub mod contiguous;
pub mod logging;
pub mod mapped;
pub mod zero;

pub trait Memory {
    fn read_u8(&mut self, address: u16) -> u8;

    fn read_u8_zp(&mut self, address: u8) -> u8 {
        self.read_u8(address as u16)
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        let l = self.read_u8(address);
        let h = self.read_u8(address.wrapping_add(1));
        u16::from_le_bytes([l, h])
    }

    fn read_u16_zp(&mut self, address: u8) -> u16 {
        let l = self.read_u8_zp(address);
        let h = self.read_u8_zp(address.wrapping_add(1));
        u16::from_le_bytes([l, h])
    }

    fn write_u8(&mut self, address: u16, value: u8);

    fn write_u8_zp(&mut self, address: u8, value: u8) {
        self.write_u8(address as u16, value)
    }

    fn write_u16(&mut self, address: u16, value: u16) {
        let [l, h] = value.to_le_bytes();
        self.write_u8(address, l);
        self.write_u8(address.wrapping_add(1), h);
    }

    fn write_u16_zp(&mut self, address: u8, value: u16) {
        let [l, h] = value.to_le_bytes();
        self.write_u8_zp(address, l);
        self.write_u8_zp(address.wrapping_add(1), h);
    }

    fn update(&mut self, cycle: usize) -> Interrupt;
}

impl<M: Memory> Memory for Box<M> {
    fn read_u8(&mut self, address: u16) -> u8 {
        (**self).read_u8(address)
    }

    fn read_u8_zp(&mut self, address: u8) -> u8 {
        (**self).read_u8_zp(address)
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        (**self).read_u16(address)
    }

    fn read_u16_zp(&mut self, address: u8) -> u16 {
        (**self).read_u16_zp(address)
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        (**self).write_u8(address, value);
    }

    fn write_u8_zp(&mut self, address: u8, value: u8) {
        (**self).write_u8_zp(address, value);
    }

    fn write_u16(&mut self, address: u16, value: u16) {
        (**self).write_u16(address, value);
    }

    fn write_u16_zp(&mut self, address: u8, value: u16) {
        (**self).write_u16_zp(address, value);
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        (**self).update(cycle)
    }
}

impl<M: Memory> Memory for Rc<RefCell<M>> {
    fn read_u8(&mut self, address: u16) -> u8 {
        self.borrow_mut().read_u8(address)
    }

    fn read_u8_zp(&mut self, address: u8) -> u8 {
        self.borrow_mut().read_u8_zp(address)
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        self.borrow_mut().read_u16(address)
    }

    fn read_u16_zp(&mut self, address: u8) -> u16 {
        self.borrow_mut().read_u16_zp(address)
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.borrow_mut().write_u8(address, value);
    }

    fn write_u8_zp(&mut self, address: u8, value: u8) {
        self.borrow_mut().write_u8_zp(address, value);
    }

    fn write_u16(&mut self, address: u16, value: u16) {
        self.borrow_mut().write_u16(address, value);
    }

    fn write_u16_zp(&mut self, address: u8, value: u16) {
        self.borrow_mut().write_u16_zp(address, value);
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        self.borrow_mut().update(cycle)
    }
}

impl<M: Memory> Memory for Arc<Mutex<M>> {
    fn read_u8(&mut self, address: u16) -> u8 {
        self.lock().unwrap().read_u8(address)
    }

    fn read_u8_zp(&mut self, address: u8) -> u8 {
        self.lock().unwrap().read_u8_zp(address)
    }

    fn read_u16(&mut self, address: u16) -> u16 {
        self.lock().unwrap().read_u16(address)
    }

    fn read_u16_zp(&mut self, address: u8) -> u16 {
        self.lock().unwrap().read_u16_zp(address)
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        self.lock().unwrap().write_u8(address, value);
    }

    fn write_u8_zp(&mut self, address: u8, value: u8) {
        self.lock().unwrap().write_u8_zp(address, value);
    }

    fn write_u16(&mut self, address: u16, value: u16) {
        self.lock().unwrap().write_u16(address, value);
    }

    fn write_u16_zp(&mut self, address: u8, value: u16) {
        self.lock().unwrap().write_u16_zp(address, value);
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        self.lock().unwrap().update(cycle)
    }
}
