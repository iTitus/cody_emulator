//! SID-like synthesizer core used by the emulator audio engine.
//!
//! The synth models three voices with basic waveform generation, ADSR-style
//! envelopes, and simple control-register semantics aligned with Cody's audio
//! register map.

use std::array;

use super::registers::AudioRegister;

/// Total number of addressable audio registers.
pub const REGISTER_COUNT: usize = 0x20;
/// Number of synth voices.
pub const VOICE_COUNT: usize = 3;
/// Register stride per voice block.
pub const VOICE_STRIDE: usize = 7;

const CTRL_GATE: u8 = 0x01;
const CTRL_SYNC: u8 = 0x02;
const CTRL_RING: u8 = 0x04;
const CTRL_TEST: u8 = 0x08;
const CTRL_TRIANGLE: u8 = 0x10;
const CTRL_SAW: u8 = 0x20;
const CTRL_PULSE: u8 = 0x40;
const CTRL_NOISE: u8 = 0x80;
const NOISE_TAPS: u16 = 0xB400;
const ENVELOPE_MAX_LEVEL_24: u32 = 0xFF_FFFF / 3;

const ATTACK_TIMES_SEC: [f32; 16] = [
    0.002, 0.008, 0.016, 0.024, 0.038, 0.056, 0.068, 0.080, 0.100, 0.250, 0.500, 0.800, 1.000,
    3.000, 5.000, 8.000,
];

const DECAY_RELEASE_TIMES_SEC: [f32; 16] = [
    0.006, 0.024, 0.048, 0.072, 0.114, 0.168, 0.204, 0.240, 0.300, 0.750, 1.500, 2.400, 3.000,
    9.000, 15.000, 24.000,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopePhase {
    Attack,
    DecaySustain,
    Release,
}

/// Internal per-voice oscillator and envelope state.
#[derive(Debug, Clone, Copy)]
struct VoiceState {
    phase: u16,
    envelope: f32,
    env_phase: EnvelopePhase,
    last_gate: bool,
    noise_wave: u16,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            phase: 0,
            envelope: 0.0,
            env_phase: EnvelopePhase::Release,
            last_gate: false,
            noise_wave: 0,
        }
    }
}

/// SID-like synth state and register file.
#[derive(Debug, Clone)]
pub struct SidLikeSynth {
    pub registers: [u8; REGISTER_COUNT],
    voices: [VoiceState; VOICE_COUNT],
    osc3_wave_readback: u8,
    noise_lfsr: u16,
}

impl Default for SidLikeSynth {
    fn default() -> Self {
        Self::new()
    }
}

impl SidLikeSynth {
    /// Creates a new synth with initial voice state.
    pub fn new() -> Self {
        let voices = array::from_fn(|_| VoiceState::default());

        Self {
            registers: [0; REGISTER_COUNT],
            voices,
            osc3_wave_readback: 0,
            noise_lfsr: 0xACE1,
        }
    }

    /// Writes a register, ignoring out-of-range and read-only targets.
    pub fn write_register(&mut self, address: u8, value: u8) {
        let index = address as usize;
        if index >= REGISTER_COUNT {
            return;
        }

        // Reject writes to read-only registers
        if address == AudioRegister::Osc3Read.as_u8() || address == AudioRegister::Env3Read.as_u8()
        {
            return;
        }

        self.registers[index] = value;
    }

    /// Reads a raw register value or `0` for out-of-range addresses.
    pub fn read_register(&self, address: u8) -> u8 {
        let index = address as usize;
        if index >= REGISTER_COUNT {
            return 0;
        }
        self.registers[index]
    }

    /// Returns current voice 3 oscillator readback value.
    pub fn osc3_readback(&self) -> u8 {
        self.osc3_wave_readback
    }

    /// Returns current voice 3 envelope readback value.
    pub fn env3_readback(&self) -> u8 {
        envelope_to_readback_u8(self.voices[2].envelope)
    }

    /// Renders a single mono sample at `sample_rate_hz`.
    pub fn render_sample(&mut self, sample_rate_hz: u32) -> f32 {
        let sample_rate_hz = sample_rate_hz.max(1) as f32; // TODO: handle limit somewhere else
        self.noise_lfsr = advance_noise_lfsr(self.noise_lfsr);

        let mut controls = [0u8; VOICE_COUNT];
        let mut wrapped = [false; VOICE_COUNT];
        let mut noise_clocked = [false; VOICE_COUNT];

        for i in 0..VOICE_COUNT {
            let base = i * VOICE_STRIDE;
            let control = self.registers[base + 4];
            controls[i] = control;

            if (control & CTRL_TEST) != 0 {
                self.voices[i].phase = 0;
                self.voices[i].envelope = 0.0;
                self.voices[i].env_phase = EnvelopePhase::Release;
                self.voices[i].last_gate = false;
                if i == 2 {
                    self.osc3_wave_readback = 0;
                }
                continue;
            }

            let freq = u16::from_le_bytes([self.registers[base], self.registers[base + 1]]);
            let phase_increment = freq >> 2;
            let previous_phase = self.voices[i].phase;
            let new_phase = previous_phase.wrapping_add(phase_increment);
            wrapped[i] = new_phase < previous_phase;
            noise_clocked[i] = ((previous_phase ^ new_phase) & 0x4000) != 0;
            self.voices[i].phase = new_phase;

            self.update_envelope(i, control, sample_rate_hz);
        }

        for i in 0..VOICE_COUNT {
            if (controls[i] & CTRL_SYNC) == 0 {
                continue;
            }

            let mod_index = match i {
                0 => 2,
                1 => 0,
                _ => 1,
            };
            if wrapped[mod_index] {
                self.voices[i].phase = 0;
            }
        }

        let mut outputs = [0.0f32; VOICE_COUNT];

        for i in 0..VOICE_COUNT {
            let base = i * VOICE_STRIDE;
            let control = controls[i];
            if (control & CTRL_TEST) != 0 {
                continue;
            }

            let mut wave_u16 = self.waveform_for_voice(i, control, base, noise_clocked[i]);

            if i == 2 {  // readback logic
                self.osc3_wave_readback = if (control & CTRL_NOISE) != 0 {
                    (self.voices[i].noise_wave >> 8) as u8
                } else {
                    (wave_u16 >> 8) as u8
                };
            }

            if (control & CTRL_RING) != 0 {
                let mod_index = match i {
                    0 => 2,
                    1 => 0,
                    _ => 1,
                };
                if (self.voices[mod_index].phase & 0x8000) != 0 {
                    wave_u16 ^= 0xFFFF;
                }
            }

            let wave = waveform_u16_to_sample(wave_u16);
            let envelope = self.voices[i].envelope.clamp(0.0, 1.0);
            outputs[i] = wave * envelope;
        }

        let volume =
            (self.registers[AudioRegister::FilterModeVolume.as_u8() as usize] & 0x0F) as f32 / 15.0;
        let mute_voice3 =
            (self.registers[AudioRegister::FilterModeVolume.as_u8() as usize] & 0x80) != 0;

        let mut mix = outputs[0] + outputs[1];
        if !mute_voice3 {
            mix += outputs[2];
        }

        (mix / 3.0 * volume).clamp(-1.0, 1.0)
    }

    /// Updates one voice ADSR state from control and AD/SR registers.
    fn update_envelope(&mut self, voice_index: usize, control: u8, sample_rate_hz: f32) {
        let base = voice_index * VOICE_STRIDE;
        let ad = self.registers[base + 5];
        let sr = self.registers[base + 6];

        let attack_nibble = (ad >> 4) as usize;
        let decay_nibble = (ad & 0x0F) as usize;
        let sustain_nibble = (sr >> 4) as usize;
        let release_nibble = (sr & 0x0F) as usize;

        let gate = (control & CTRL_GATE) != 0;
        let voice = &mut self.voices[voice_index];

        if gate && !voice.last_gate {
            voice.env_phase = EnvelopePhase::Attack;
        } else if !gate && voice.last_gate {
            voice.env_phase = EnvelopePhase::Release;
        }
        voice.last_gate = gate;

        let attack_step = envelope_step(ATTACK_TIMES_SEC[attack_nibble], sample_rate_hz);
        let decay_step = envelope_step(DECAY_RELEASE_TIMES_SEC[decay_nibble], sample_rate_hz);
        let release_step = envelope_step(DECAY_RELEASE_TIMES_SEC[release_nibble], sample_rate_hz);
        let sustain_level = sustain_nibble as f32 / 15.0;

        match voice.env_phase {
            EnvelopePhase::Attack => {
                voice.envelope += attack_step;
                if voice.envelope >= 1.0 {
                    voice.envelope = 1.0;
                    voice.env_phase = EnvelopePhase::DecaySustain;
                }
            }
            EnvelopePhase::DecaySustain => {
                if voice.envelope > sustain_level {
                    voice.envelope = (voice.envelope - decay_step).max(sustain_level);
                }
            }
            EnvelopePhase::Release => {
                voice.envelope = (voice.envelope - release_step).max(0.0);
            }
        }
    }

    /// Produces one raw waveform sample for a voice before envelope scaling.
    fn waveform_for_voice(
        &mut self,
        voice_index: usize,
        control: u8,
        base: usize,
        noise_clocked: bool,
    ) -> u16 {
        let phase = self.voices[voice_index].phase;

        if (control & CTRL_TRIANGLE) != 0 {
            return triangle(phase);
        }
        if (control & CTRL_SAW) != 0 {
            return saw(phase);
        }
        if (control & CTRL_PULSE) != 0 {
            let pulse =
                ((self.registers[base + 3] as u16 & 0x0F) << 8) | self.registers[base + 2] as u16;
            let threshold = pulse << 4;
            return if phase < threshold { 0xFFFF } else { 0x0000 };
        }
        if (control & CTRL_NOISE) != 0 {
            let voice = &mut self.voices[voice_index];
            if noise_clocked {
                voice.noise_wave = self.noise_lfsr;
            }
            return voice.noise_wave;
        }

        0
    }
}

/// Converts an envelope segment duration to a per-sample delta.
fn envelope_step(seconds: f32, sample_rate_hz: f32) -> f32 {
    if seconds <= 0.0 || sample_rate_hz <= 0.0 {
        1.0
    } else {
        (1.0 / (seconds * sample_rate_hz)).clamp(0.000_000_1, 1.0)
    }
}

/// Triangle waveform from 16-bit phase.
fn triangle(phase: u16) -> u16 {
    let p = phase as u32;
    let value = if p < 32768 { p * 2 } else { (65535 - p) * 2 };
    value as u16
}

/// Sawtooth waveform from 16-bit phase.
fn saw(phase: u16) -> u16 {
    phase
}

/// Converts unsigned waveform domain into bipolar sample domain.
fn waveform_u16_to_sample(wave: u16) -> f32 {
    wave as f32 / 32767.5 - 1.0
}

/// Converts normalized envelope [0,1] into SPIN-compatible Env3 readback byte.
fn envelope_to_readback_u8(envelope: f32) -> u8 {
    let amplitude_24 = (envelope.clamp(0.0, 1.0) * ENVELOPE_MAX_LEVEL_24 as f32) as u32;
    (amplitude_24 >> 16) as u8
}

fn advance_noise_lfsr(noise: u16) -> u16 {
    let feedback = if (noise & 1) != 0 { NOISE_TAPS } else { 0 };
    (noise >> 1) ^ feedback
}
