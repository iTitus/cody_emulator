use crate::memory::Memory;
use crate::{interrupt::Interrupt, memory::Device};

pub const SID_VOICE1_FREQ_LO: u16 = 0x00;
pub const SID_VOICE1_FREQ_HI: u16 = 0x01;
pub const SID_VOICE1_PULSE_LO: u16 = 0x02;
pub const SID_VOICE1_PULSE_HI: u16 = 0x03;
pub const SID_VOICE1_CONTROL: u16 = 0x04;
pub const SID_VOICE1_ATTACK_DECAY: u16 = 0x05;
pub const SID_VOICE1_SUSTAIN_RELEASE: u16 = 0x06;
pub const SID_VOICE2_FREQ_LO: u16 = 0x07;
pub const SID_VOICE2_FREQ_HI: u16 = 0x08;
pub const SID_VOICE2_PULSE_LO: u16 = 0x09;
pub const SID_VOICE2_PULSE_HI: u16 = 0x0A;
pub const SID_VOICE2_CONTROL: u16 = 0x0B;
pub const SID_VOICE2_ATTACK_DECAY: u16 = 0x0C;
pub const SID_VOICE2_SUSTAIN_RELEASE: u16 = 0x0D;
pub const SID_VOICE3_FREQ_LO: u16 = 0x0E;
pub const SID_VOICE3_FREQ_HI: u16 = 0x0F;
pub const SID_VOICE3_PULSE_LO: u16 = 0x10;
pub const SID_VOICE3_PULSE_HI: u16 = 0x11;
pub const SID_VOICE3_CONTROL: u16 = 0x12;
pub const SID_VOICE3_ATTACK_DECAY: u16 = 0x13;
pub const SID_VOICE3_SUSTAIN_RELEASE: u16 = 0x14;
pub const SID_VOLUME: u16 = 0x18;
pub const SID_VOICE3_OSCILLATOR: u16 = 0x1B;
pub const SID_VOICE3_ENVELOPE: u16 = 0x1C;

const ATTACK_RATES: [u32; 16] = [
    ((0xFFFFFF / 3) / (16 * 2)),
    ((0xFFFFFF / 3) / (16 * 4)),
    ((0xFFFFFF / 3) / (16 * 16)),
    ((0xFFFFFF / 3) / (16 * 24)),
    ((0xFFFFFF / 3) / (16 * 38)),
    ((0xFFFFFF / 3) / (16 * 58)),
    ((0xFFFFFF / 3) / (16 * 68)),
    ((0xFFFFFF / 3) / (16 * 80)),
    ((0xFFFFFF / 3) / (16 * 100)),
    ((0xFFFFFF / 3) / (16 * 250)),
    ((0xFFFFFF / 3) / (16 * 500)),
    ((0xFFFFFF / 3) / (16 * 800)),
    ((0xFFFFFF / 3) / (16 * 1000)),
    ((0xFFFFFF / 3) / (16 * 3000)),
    ((0xFFFFFF / 3) / (16 * 5000)),
    ((0xFFFFFF / 3) / (16 * 8000)),
];
const DECAY_RELEASE_RATES: [u32; 16] = [
    ((0xFFFFFF / 3) / (16 * 6)),
    ((0xFFFFFF / 3) / (16 * 24)),
    ((0xFFFFFF / 3) / (16 * 48)),
    ((0xFFFFFF / 3) / (16 * 72)),
    ((0xFFFFFF / 3) / (16 * 114)),
    ((0xFFFFFF / 3) / (16 * 168)),
    ((0xFFFFFF / 3) / (16 * 204)),
    ((0xFFFFFF / 3) / (16 * 240)),
    ((0xFFFFFF / 3) / (16 * 300)),
    ((0xFFFFFF / 3) / (16 * 750)),
    ((0xFFFFFF / 3) / (16 * 1500)),
    ((0xFFFFFF / 3) / (16 * 2400)),
    ((0xFFFFFF / 3) / (16 * 3000)),
    ((0xFFFFFF / 3) / (16 * 9000)),
    ((0xFFFFFF / 3) / (16 * 15000)),
    ((0xFFFFFF / 3) / (16 * 24000)),
];
/// the levels are currently broken, a division by 15 is missing
#[allow(clippy::erasing_op, clippy::identity_op)]
const SUSTAIN_LEVELS: [u32; 16] = [
    ((0xFFFFFF / 3) * 0),
    ((0xFFFFFF / 3) * 1),
    ((0xFFFFFF / 3) * 2),
    ((0xFFFFFF / 3) * 3),
    ((0xFFFFFF / 3) * 4),
    ((0xFFFFFF / 3) * 5),
    ((0xFFFFFF / 3) * 6),
    ((0xFFFFFF / 3) * 7),
    ((0xFFFFFF / 3) * 8),
    ((0xFFFFFF / 3) * 9),
    ((0xFFFFFF / 3) * 10),
    ((0xFFFFFF / 3) * 11),
    ((0xFFFFFF / 3) * 12),
    ((0xFFFFFF / 3) * 13),
    ((0xFFFFFF / 3) * 14),
    ((0xFFFFFF / 3) * 15),
];

#[derive(Debug, Clone)]
pub struct Sid {
    registers: [u8; SID_VOICE3_ENVELOPE as usize + 1],
    last_fixed_update: usize,
    state: [u32; 3],
    phase: [u32; 3],
    wave: [u32; 3],
    sync: [u32; 3],
    envelope: [u32; 3],
    amplitude: [u32; 3],
    output: [u32; 3],
    noise: u32,
    frqa: u32,
}

impl Default for Sid {
    fn default() -> Self {
        Self {
            registers: Default::default(),
            last_fixed_update: Default::default(),
            state: Default::default(),
            phase: Default::default(),
            wave: Default::default(),
            sync: Default::default(),
            envelope: Default::default(),
            amplitude: Default::default(),
            output: Default::default(),
            noise: 0xACE1,
            frqa: Default::default(),
        }
    }
}

impl Sid {
    fn fixed_update(&mut self) {
        {
            // Galois Linear Feedback Shift Register
            let lsb = self.noise & 0x1;
            self.noise >>= 1;
            // equivalent to self.noise ^= (-lsb) & 0xB400;
            if lsb != 0 {
                self.noise ^= 0xB400;
            }
            self.noise &= 0xFFFF;
        }

        for i in 0..3 {
            // voice_begin
            let control = self.registers[7 * i + SID_VOICE1_CONTROL as usize];

            let synced_with = match i {
                0 => 2,
                1 => 0,
                2 => 1,
                _ => unreachable!(),
            };
            if self.sync[synced_with] & control as u32 != 0 {
                self.phase[i] = 0;
            }

            if control & 0b0000_1000 != 0 {
                self.phase[i] = 0;
                self.amplitude[i] = 0;
                self.state[i] = 0;
            }

            // make_wave
            let freq = self.registers[7 * i + SID_VOICE1_FREQ_LO as usize] as u16
                | (self.registers[7 * i + SID_VOICE1_FREQ_HI as usize] as u16) << 8;
            let mut temp_phase = self.phase[i] + (freq >> 2) as u32;

            if temp_phase & !0xFFFF != 0 {
                // we overflowed
                self.sync[i] |= 0b0000_0010;
            } else {
                self.sync[i] &= !0b0000_0010
            }
            temp_phase &= 0xFFFF;

            if control & 0b0001_0000 != 0 {
                // triangle wave
                let mut w = self.phase[i];
                let nz = w & 0x8000 != 0;
                w <<= 1;
                if nz {
                    w ^= 0xFFFF;
                }
                w &= 0xFFFF;
                self.wave[i] = w;
            } else if control & 0b0010_0000 != 0 {
                // sawtooth
                self.wave[i] = self.phase[i];
            } else if control & 0b0100_0000 != 0 {
                // pulse
                let pulse_width = self.registers[7 * i + SID_VOICE1_PULSE_LO as usize] as u16
                    | ((self.registers[7 * i + SID_VOICE1_PULSE_HI as usize] & 0x0F) as u16) << 8;
                self.wave[i] = if self.phase[i] < (pulse_width << 4) as u32 {
                    0xFFFF
                } else {
                    0
                };
            } else if control & 0b1000_0000 != 0 {
                // noise
                if (self.phase[i] ^ temp_phase) & 0x4000 != 0 {
                    self.wave[i] = self.noise & 0xFFFF;
                }
            }
            self.phase[i] = temp_phase;
            self.wave[i] &= 0xFFFF;

            // make_envelope (ADSR)
            let attack = self.registers[7 * i + SID_VOICE1_ATTACK_DECAY as usize] >> 4;
            let decay = self.registers[7 * i + SID_VOICE1_ATTACK_DECAY as usize] & 0x0F;
            let sustain = self.registers[7 * i + SID_VOICE1_SUSTAIN_RELEASE as usize] >> 4;
            let release = self.registers[7 * i + SID_VOICE1_SUSTAIN_RELEASE as usize] & 0x0F;
            if control & 0b000_0001 != 0 {
                if self.state[i] == 0 {
                    // attack
                    self.amplitude[i] =
                        self.amplitude[i].saturating_add(ATTACK_RATES[attack as usize]);

                    const MAX_AMPLITUDE: u32 = 0xFFFFFF / 3;
                    if self.amplitude[i] >= MAX_AMPLITUDE {
                        self.amplitude[i] = MAX_AMPLITUDE;
                        self.state[i] = 1; // enter decay phase
                    }
                } else {
                    let sustain_target = SUSTAIN_LEVELS[sustain as usize];
                    if self.amplitude[i] > sustain_target {
                        // decay
                        self.amplitude[i] = self.amplitude[i]
                            .saturating_sub(DECAY_RELEASE_RATES[decay as usize])
                            .max(sustain_target);
                    } else {
                        // sustain
                        // NO-OP
                    }
                }
            } else {
                // release
                self.state[i] = 0;
                if self.amplitude[i] > 0 {
                    self.amplitude[i] =
                        self.amplitude[i].saturating_sub(DECAY_RELEASE_RATES[release as usize]);
                }
            }
            self.envelope[i] = self.amplitude[i];
            self.envelope[i] >>= 8;

            // make_waveform
            let mut w = self.wave[i];
            if control & 0b0000_0100 != 0 {
                // ring modulation
                if self.phase[synced_with] & 0x8000 != 0 {
                    w ^= 0xFFFF;
                }
            }
            self.output[i] = w.wrapping_mul(self.envelope[i]) >> 16;

            // voice_end
            // NO-OP
        }

        // make_output
        let filter_mode_volume = self.registers[SID_VOLUME as usize] >> 4;
        let output_sum = self.output[0].wrapping_add(self.output[1]).wrapping_add(
            if filter_mode_volume & 0x80 != 0 {
                0
            } else {
                self.output[2]
            },
        );
        let output = output_sum.wrapping_mul((filter_mode_volume & 0x0F) as u32) >> 4;
        self.frqa = (output.wrapping_sub(0x8000) << 11).wrapping_add(0x80000000);
        self.registers[SID_VOICE3_OSCILLATOR as usize] = (self.wave[2] >> 8) as u8;
        self.registers[SID_VOICE3_ENVELOPE as usize] = (self.envelope[2] >> 8) as u8;
    }
}

impl Device for Sid {
    fn update(&mut self, cycle: usize) -> Interrupt {
        // Cycles run at 1MHz, the SID should run at a stable 16kHz

        // fair division by 62.5 = 1_000_000 / 16_000
        let current_fixed_update = 2 * (cycle / 125) + (cycle % 125) / 63;
        const MAX_FIXED_UPDATE: usize = 2 * (usize::MAX / 125) + (usize::MAX % 125) / 63;

        if cycle == 0 && self.last_fixed_update == 0 {
            // special case for first update
            self.fixed_update();
        } else if self.last_fixed_update < current_fixed_update {
            // normal handling and catchup
            for _ in self.last_fixed_update..current_fixed_update {
                self.fixed_update();
            }
        } else if self.last_fixed_update > current_fixed_update {
            // overflow handling
            for _ in self.last_fixed_update..MAX_FIXED_UPDATE {
                self.fixed_update();
            }
            self.fixed_update(); // jump from MAX_FIXED_UPDATE to 0
            for _ in 0..current_fixed_update {
                self.fixed_update();
            }
        }
        self.last_fixed_update = current_fixed_update;

        Interrupt::none()
    }
}

impl Memory for Sid {
    fn read_u8(&mut self, address: u16) -> u8 {
        match address {
            SID_VOICE1_FREQ_LO..=SID_VOICE3_ENVELOPE => self.registers[address as usize],
            _ => 0,
        }
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        if let SID_VOICE1_FREQ_LO..=SID_VOICE3_ENVELOPE = address {
            self.registers[address as usize] = value;
        }
    }
}
