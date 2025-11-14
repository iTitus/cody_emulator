use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub initial: Configuration,
    pub r#final: Configuration,
    pub cycles: Vec<Cycle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    pub pc: u16,
    pub s: u8,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub p: u8,
    pub ram: Vec<RamValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle(u16, u8, CycleOp);

impl Cycle {
    pub fn address(&self) -> u16 {
        self.0
    }

    pub fn value(&self) -> u8 {
        self.1
    }

    pub fn op(&self) -> CycleOp {
        self.2
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleOp {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamValue(u16, u8);

impl RamValue {
    pub fn address(&self) -> u16 {
        self.0
    }

    pub fn value(&self) -> u8 {
        self.1
    }
}
