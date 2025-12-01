use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub mod keyboard;
pub mod uart;
pub mod via;
pub mod vid;

pub trait MemoryDevice {
    fn read(&mut self, address: u16) -> Option<u8>;

    fn write(&mut self, address: u16, value: u8) -> Option<()>;

    fn tick(&mut self) {}
}

impl<M: MemoryDevice> MemoryDevice for Rc<RefCell<M>> {
    fn read(&mut self, address: u16) -> Option<u8> {
        self.borrow_mut().read(address)
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        self.borrow_mut().write(address, value)
    }

    fn tick(&mut self) {
        self.borrow_mut().tick()
    }
}

impl<M: MemoryDevice> MemoryDevice for Arc<Mutex<M>> {
    fn read(&mut self, address: u16) -> Option<u8> {
        self.lock().unwrap().read(address)
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        self.lock().unwrap().write(address, value)
    }

    fn tick(&mut self) {
        self.lock().unwrap().tick()
    }
}
