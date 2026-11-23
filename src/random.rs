const INITIAL_STATE: u16 = 0xACE1;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Random {
    state: u16,
}

impl Random {
    pub const fn new() -> Self {
        Self::new_with_state(INITIAL_STATE)
    }

    pub const fn new_with_state(state: u16) -> Self {
        Self { state }
    }

    pub const fn next_u16(&mut self) -> u16 {
        // Galois Linear Feedback Shift Register
        let mut state = self.state;
        let lsb_set: bool = (state & 0x1) != 0;
        state >>= 1;
        // equivalent to state ^= (-lsb) & 0xB400;
        if lsb_set {
            state ^= 0xB400;
        }
        self.state = state;
        state
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}
