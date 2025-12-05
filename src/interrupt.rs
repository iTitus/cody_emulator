#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Interrupt {
    irq: bool,
    nmi: bool,
}

impl Interrupt {
    pub fn none() -> Self {
        Self {
            irq: false,
            nmi: false,
        }
    }

    pub fn irq() -> Self {
        Self {
            irq: true,
            nmi: false,
        }
    }

    pub fn nmi() -> Self {
        Self {
            irq: false,
            nmi: true,
        }
    }

    pub fn is_irq(&self) -> bool {
        self.irq
    }

    pub fn is_nmi(&self) -> bool {
        self.nmi
    }

    pub fn or(self, other: Self) -> Self {
        Self {
            irq: self.irq | other.irq,
            nmi: self.nmi | other.nmi,
        }
    }
}

impl Default for Interrupt {
    fn default() -> Self {
        Self::none()
    }
}
