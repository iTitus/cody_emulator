use crate::interrupt::Interrupt;
use crate::memory::Memory;

#[derive(Debug, Clone, Default)]
pub struct BlankingRegister {
    in_blanking_interval: bool,
}

impl Memory for BlankingRegister {
    fn read_u8(&mut self, _address: u16) -> u8 {
        if self.in_blanking_interval { 1 } else { 0 }
    }

    fn write_u8(&mut self, _address: u16, _value: u8) {}

    fn update(&mut self, cycle: usize) -> Interrupt {
        // one (half-)frame is rendered roughly every 60 Hz
        const FPS: f64 = 60.0 / 1.001;
        // 262 lines in a (half-)frame
        // 9 lines for VSYNC
        // 12 blank lines
        // 220 lines (20 lines top border + 200 (25x8) screen area) | VBLANK=0
        // 21 lines bottom border
        const VBLANK1_RATIO: f64 = (9.0 + 12.0 + 21.0) / 262.0;
        const FRAME_TIME: f64 = 1.0 / FPS;
        const VBLANK1_TIME: f64 = VBLANK1_RATIO * FRAME_TIME;
        // Propeller runs at 80 MHz and the WD65C02 runs at 1MHz
        const CYCLE_FREQUENCY: f64 = 1000000.0;
        const FRAME_CYCLES: usize = (FRAME_TIME * CYCLE_FREQUENCY) as usize;
        const VBLANK1_CYCLES: usize = (VBLANK1_TIME * CYCLE_FREQUENCY) as usize;

        let frame_cycle = cycle % FRAME_CYCLES;
        self.in_blanking_interval = frame_cycle < VBLANK1_CYCLES;
        Interrupt::none()
    }
}
