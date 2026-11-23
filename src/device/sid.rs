use crate::memory::Memory;
use crate::random::Random;
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

pub const VOICE_COUNT: usize = 3;

const SAMPLE_RATE: u32 = 16_000;
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
#[expect(clippy::erasing_op, clippy::identity_op)]
const SUSTAIN_LEVELS: [u32; 16] = [
    ((0xFFFFFF / 3) * 0 / 15),
    ((0xFFFFFF / 3) * 1 / 15),
    ((0xFFFFFF / 3) * 2 / 15),
    ((0xFFFFFF / 3) * 3 / 15),
    ((0xFFFFFF / 3) * 4 / 15),
    ((0xFFFFFF / 3) * 5 / 15),
    ((0xFFFFFF / 3) * 6 / 15),
    ((0xFFFFFF / 3) * 7 / 15),
    ((0xFFFFFF / 3) * 8 / 15),
    ((0xFFFFFF / 3) * 9 / 15),
    ((0xFFFFFF / 3) * 10 / 15),
    ((0xFFFFFF / 3) * 11 / 15),
    ((0xFFFFFF / 3) * 12 / 15),
    ((0xFFFFFF / 3) * 13 / 15),
    ((0xFFFFFF / 3) * 14 / 15),
    ((0xFFFFFF / 3) * 15 / 15),
];

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
enum VoiceState {
    #[default]
    Attack,
    Decay,
}

#[derive(Debug, Default)]
struct Voice {
    state: VoiceState,
    phase: u16,
    wave: u16,
    sync: u8,
    envelope: u16,
    amplitude: u32,
}

#[derive(Default)]
pub struct Sid {
    registers: [u8; SID_VOICE3_ENVELOPE as usize + 1],
    last_fixed_update: usize,
    rng: Random,
    voices: [Voice; 3],
    #[cfg(feature = "sound")]
    device: Option<rodio::MixerDeviceSink>,
    #[cfg(feature = "sound")]
    producer: Option<<ringbuf::HeapRb<i16> as ringbuf::traits::Split>::Prod>,
    #[cfg(feature = "sound")]
    dc_blocker_parameter: Option<rodio::Sample>,
}

impl Sid {
    pub fn new_without_output() -> Self {
        Self::default()
    }

    #[cfg(feature = "sound")]
    pub fn new_with_output(
        dc_blocker_parameter: Option<rodio::Sample>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut device = rodio::DeviceSinkBuilder::open_default_sink()?;
        device.log_on_drop(false);

        Ok(Self {
            device: Some(device),
            dc_blocker_parameter,
            ..Self::new_without_output()
        })
    }

    fn startup(&mut self) {
        #[cfg(feature = "sound")]
        {
            use ringbuf::traits::Split;

            if let Some(device) = &mut self.device {
                // one second of buffer
                let rb = ringbuf::HeapRb::<i16>::new(SAMPLE_RATE as usize);
                let (producer, consumer) = rb.split();
                self.producer = Some(producer);

                let source = rodio_output::MonoRbSource::new_with_fallback(consumer, i16::MIN);
                if let Some(dc_blocker_parameter) = self.dc_blocker_parameter {
                    device.mixer().add(rodio_output::MonoDcBlocker::new(
                        source,
                        dc_blocker_parameter,
                    ));
                } else {
                    device.mixer().add(source);
                }
            }
        }
    }

    fn fixed_update(&mut self) {
        let noise = self.rng.next_u16();

        let mut voice_outputs = [0; VOICE_COUNT];
        for i in 0..VOICE_COUNT {
            // voice_begin
            let control = self.registers[7 * i + SID_VOICE1_CONTROL as usize];

            let synced_with = match i {
                0 => 2,
                1 => 0,
                2 => 1,
                _ => unreachable!(),
            };
            let [voice, synced_with_voice] = self
                .voices
                .get_disjoint_mut([i, synced_with])
                .expect("valid borrows");

            if synced_with_voice.sync & control != 0 {
                voice.phase = 0;
            }

            if control & 0b0000_1000 != 0 {
                voice.phase = 0;
                voice.amplitude = 0;
                voice.state = VoiceState::Attack;
            }

            // make_wave
            let freq = self.registers[7 * i + SID_VOICE1_FREQ_LO as usize] as u16
                | (self.registers[7 * i + SID_VOICE1_FREQ_HI as usize] as u16) << 8;
            let temp_phase = {
                let (temp_phase, overflow) = voice.phase.overflowing_add(freq >> 2);
                if overflow {
                    voice.sync |= 0b0000_0010;
                } else {
                    voice.sync &= !0b0000_0010
                }
                temp_phase
            };

            if control & 0b0001_0000 != 0 {
                // triangle wave
                let mut w = voice.phase;
                let nz = w & 0x8000 != 0;
                w <<= 1;
                if nz {
                    w ^= 0xFFFF;
                }
                voice.wave = w;
            } else if control & 0b0010_0000 != 0 {
                // sawtooth
                voice.wave = voice.phase;
            } else if control & 0b0100_0000 != 0 {
                // pulse
                let pulse_width = self.registers[7 * i + SID_VOICE1_PULSE_LO as usize] as u16
                    | ((self.registers[7 * i + SID_VOICE1_PULSE_HI as usize] & 0x0F) as u16) << 8;
                voice.wave = if voice.phase < pulse_width << 4 {
                    0xFFFF
                } else {
                    0
                };
            } else if control & 0b1000_0000 != 0 {
                // noise
                if (voice.phase ^ temp_phase) & 0x4000 != 0 {
                    voice.wave = noise;
                }
            }
            voice.phase = temp_phase;

            // make_envelope (ADSR)
            let attack = self.registers[7 * i + SID_VOICE1_ATTACK_DECAY as usize] >> 4;
            let decay = self.registers[7 * i + SID_VOICE1_ATTACK_DECAY as usize] & 0x0F;
            let sustain = self.registers[7 * i + SID_VOICE1_SUSTAIN_RELEASE as usize] >> 4;
            let release = self.registers[7 * i + SID_VOICE1_SUSTAIN_RELEASE as usize] & 0x0F;
            if control & 0b000_0001 != 0 {
                match voice.state {
                    VoiceState::Attack => {
                        voice.amplitude = voice
                            .amplitude
                            .saturating_add(ATTACK_RATES[attack as usize]);

                        const MAX_AMPLITUDE: u32 = 0xFFFFFF / 3;
                        if voice.amplitude >= MAX_AMPLITUDE {
                            voice.amplitude = MAX_AMPLITUDE;
                            voice.state = VoiceState::Decay;
                        }
                    }
                    VoiceState::Decay => {
                        let level_sustain = SUSTAIN_LEVELS[sustain as usize];
                        if level_sustain < voice.amplitude {
                            // decay
                            voice.amplitude = voice
                                .amplitude
                                .saturating_sub(DECAY_RELEASE_RATES[decay as usize])
                                .max(level_sustain);
                        }
                    }
                }
            } else {
                // release
                voice.state = VoiceState::Attack;
                if voice.amplitude != 0 {
                    voice.amplitude = voice
                        .amplitude
                        .saturating_sub(DECAY_RELEASE_RATES[release as usize]);
                }
            }
            voice.envelope = (voice.amplitude >> 8) as u16;

            // make_waveform
            let mut w = voice.wave;
            if control & 0b0000_0100 != 0 {
                // ring modulation
                if synced_with_voice.phase & 0x8000 != 0 {
                    w ^= 0xFFFF;
                }
            }

            voice_outputs[i] = ((w as u32).wrapping_mul(voice.envelope as u32) >> 16) as u16;
        }

        #[cfg(feature = "sound")]
        {
            use log::warn;
            use ringbuf::traits::Producer;

            // make_output
            let filter_mode_volume = self.registers[SID_VOLUME as usize];
            let output_sum = (voice_outputs[0] as u32)
                .wrapping_add(voice_outputs[1] as u32)
                .wrapping_add(if filter_mode_volume & 0x80 != 0 {
                    // voice 3 is skipped if bit is set
                    0
                } else {
                    voice_outputs[2] as u32
                });
            let output = output_sum.wrapping_mul((filter_mode_volume & 0x0F) as u32) >> 4;

            if let Some(producer) = &mut self.producer {
                let r = producer.try_push(output.wrapping_sub(0x8000) as i16);
                if r.is_err() {
                    warn!("audio buffer overrun");
                }
            }
        }

        self.registers[SID_VOICE3_OSCILLATOR as usize] = (self.voices[2].wave >> 8) as u8;
        self.registers[SID_VOICE3_ENVELOPE as usize] = (self.voices[2].envelope >> 8) as u8;
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
            self.startup();
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

#[cfg(feature = "sound")]
mod rodio_output {
    use super::*;
    use log::warn;
    use ringbuf::traits::Consumer;
    use std::time::Duration;

    pub struct MonoRbSource {
        inner: <ringbuf::HeapRb<i16> as ringbuf::traits::Split>::Cons,
        fallback: i16,
    }

    impl MonoRbSource {
        pub fn new_with_fallback(
            source: <ringbuf::HeapRb<i16> as ringbuf::traits::Split>::Cons,
            fallback: i16,
        ) -> Self {
            Self {
                inner: source,
                fallback,
            }
        }
    }

    impl Iterator for MonoRbSource {
        type Item = rodio::Sample;

        fn next(&mut self) -> Option<Self::Item> {
            // 0 is the equilibrium value (silence)
            Some(
                self.inner.try_pop().unwrap_or_else(|| {
                    warn!("audio buffer underrun");
                    self.fallback
                }) as rodio::Sample
                    / 32_768.0,
            )
        }
    }

    impl rodio::Source for MonoRbSource {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(1)
        }

        fn sample_rate(&self) -> rodio::SampleRate {
            const SR: rodio::SampleRate = rodio::SampleRate::new(SAMPLE_RATE).expect("non-zero");
            SR
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    pub struct MonoDcBlocker<S> {
        inner: S,
        p: rodio::Sample,
        last_input: Option<rodio::Sample>,
        last_output: rodio::Sample,
    }

    impl<S: rodio::Source> MonoDcBlocker<S> {
        pub fn new(source: S, p: rodio::Sample) -> Self {
            assert_eq!(source.channels().get(), 1);
            Self {
                inner: source,
                p,
                last_input: None,
                last_output: 0.0,
            }
        }
    }

    impl<S: rodio::Source> Iterator for MonoDcBlocker<S> {
        type Item = rodio::Sample;

        fn next(&mut self) -> Option<Self::Item> {
            let last_input = if let Some(last_input) = self.last_input {
                last_input
            } else {
                self.inner.next()?
            };
            let input = self.inner.next()?;
            let output = (self.p * self.last_output + (input - last_input)).clamp(-1.0, 1.0);
            self.last_input = Some(input);
            self.last_output = output;
            Some(output)
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            let (lower, upper) = self.inner.size_hint();
            (lower.saturating_sub(1), upper.map(|x| x.saturating_sub(1)))
        }
    }

    impl<S: rodio::Source + ExactSizeIterator> ExactSizeIterator for MonoDcBlocker<S> {}

    impl<S: rodio::Source> rodio::Source for MonoDcBlocker<S> {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> rodio::ChannelCount {
            rodio::nz!(1)
        }

        fn sample_rate(&self) -> rodio::SampleRate {
            self.inner.sample_rate()
        }

        fn total_duration(&self) -> Option<Duration> {
            self.inner.total_duration().map(|d| {
                const NANOS_PER_SECOND: u64 = Duration::from_secs(1).as_nanos() as u64;

                d.saturating_sub(Duration::from_nanos(
                    NANOS_PER_SECOND / self.sample_rate().get() as u64,
                ))
            })
        }
    }
}
