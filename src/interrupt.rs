#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Interrupt {
    None,
    Nmi,
    Irq,
}
