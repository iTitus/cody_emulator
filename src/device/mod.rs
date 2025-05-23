use std::sync::{Arc, Mutex};

pub mod via;
pub mod vid;

pub trait MemoryDevice {
    fn read(&mut self, address: u16) -> Option<u8>;

    fn write(&mut self, address: u16, value: u8) -> Option<()>;

    fn on_cycle(&mut self) {}

    fn on_instruction_finished(&mut self) {}
}

impl<M: MemoryDevice> MemoryDevice for Arc<Mutex<M>> {
    fn read(&mut self, address: u16) -> Option<u8> {
        self.lock().unwrap().read(address)
    }

    fn write(&mut self, address: u16, value: u8) -> Option<()> {
        self.lock().unwrap().write(address, value)
    }
}
