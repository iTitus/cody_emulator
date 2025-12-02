use std::sync::{Arc, Mutex};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Interrupt {
    None,
    Nmi,
    Irq,
}

pub trait InterruptProvider {
    fn consume_irq(&mut self) -> bool;
    fn consume_nmi(&mut self) -> bool;
}

pub trait InterruptTrigger {
    fn trigger_irq(&mut self);
    fn trigger_nmi(&mut self);
}

impl<IP: InterruptProvider> InterruptProvider for Arc<Mutex<IP>> {
    fn consume_irq(&mut self) -> bool {
        self.lock().unwrap().consume_irq()
    }

    fn consume_nmi(&mut self) -> bool {
        self.lock().unwrap().consume_nmi()
    }
}

impl<IP: InterruptTrigger> InterruptTrigger for Arc<Mutex<IP>> {
    fn trigger_irq(&mut self) {
        self.lock().unwrap().trigger_irq();
    }

    fn trigger_nmi(&mut self) {
        self.lock().unwrap().trigger_nmi();
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct NoopInterruptProvider;

impl InterruptProvider for NoopInterruptProvider {
    fn consume_irq(&mut self) -> bool {
        false
    }

    fn consume_nmi(&mut self) -> bool {
        false
    }
}

impl InterruptTrigger for NoopInterruptProvider {
    fn trigger_irq(&mut self) {}

    fn trigger_nmi(&mut self) {}
}

#[derive(Debug, Copy, Clone, Default)]
pub struct SimpleInterruptProvider {
    irq: bool,
    nmi: bool,
}

impl InterruptProvider for SimpleInterruptProvider {
    fn consume_irq(&mut self) -> bool {
        let value = self.irq;
        self.irq = false;
        value
    }

    fn consume_nmi(&mut self) -> bool {
        let value = self.nmi;
        self.nmi = false;
        value
    }
}

impl InterruptTrigger for SimpleInterruptProvider {
    fn trigger_irq(&mut self) {
        self.irq = true;
    }

    fn trigger_nmi(&mut self) {
        self.nmi = true;
    }
}
