//! MMIO facade for the SID-like audio block.
//!
//! The CPU interacts with it through the memory bus at `$D400`.
//! Writes are queued as cycle-stamped events and reads of special readback
//! registers are resolved through the audio engine.

use crate::device::audio::engine::{AudioEngine, AudioEvent, AudioTiming, SharedAudioDataPlane};
use crate::device::audio::engine::SharedAudioControlPlane;
use crate::device::audio::factory::{
    create_audio_control_plane,
    create_audio_data_plane,
    create_audio_engine,
};
use crate::device::audio::AudioConfig;
use crate::device::audio::compute_soft_cap_samples;
use crate::device::audio::synth::AudioRegister;
use crate::interrupt::Interrupt;
use crate::memory::Memory;

/// Base MMIO address for the audio register block.
pub const AUDIO_BASE: u16 = 0xD400;
/// Number of exposed MMIO registers in the block.
pub const AUDIO_REGISTER_COUNT: u16 = 0x20;

const DEFAULT_PCM_CAPACITY: usize = 4096;
const DEFAULT_WRITE_QUEUE_CAPACITY: usize = 8192;
const DEFAULT_CPU_HZ: f64 = 1_000_000.0;
const DEFAULT_SYNTH_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_TARGET_LATENCY_SAMPLES: u32 = 128;  // 8ms of buffering @ 16kHz

/// Memory-mapped audio device bridging CPU bus traffic and audio runtime.
#[derive(Debug, Clone)]
pub struct AudioMmioDevice {
    registers: [u8; AUDIO_REGISTER_COUNT as usize],
    runtime: SharedAudioDataPlane,
    control_queues: SharedAudioControlPlane,
    engine: AudioEngine,
    last_cycle: usize,
    last_osc3: u8,
    last_env3: u8,
}

impl Default for AudioMmioDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioMmioDevice {
    /// Creates an MMIO audio device using default timing parameters.
    pub fn new() -> Self {
        // Convert target latency from samples to cycles
        let cycles_per_sample = DEFAULT_CPU_HZ / DEFAULT_SYNTH_SAMPLE_RATE as f64;
        let target_latency_cycles = cycles_per_sample * DEFAULT_TARGET_LATENCY_SAMPLES as f64;
        let config = AudioConfig::new(
            DEFAULT_CPU_HZ,
            DEFAULT_SYNTH_SAMPLE_RATE,
            target_latency_cycles,
        );
        Self::with_timing(config)
    }

    /// Creates an MMIO audio device with explicit timing parameters.
    /// target_latency_cycles: desired audio buffer latency in CPU cycles
    pub fn with_timing(config: AudioConfig) -> Self {
        let runtime = create_audio_data_plane(DEFAULT_PCM_CAPACITY);
        let control_queues = create_audio_control_plane(DEFAULT_WRITE_QUEUE_CAPACITY);
        let engine = create_audio_engine(
            runtime.clone(),
            control_queues.clone(),
            config,
        );

        let timing = AudioTiming::new(config);
        let soft_cap = compute_soft_cap_samples(
            timing.target_latency_samples,
            runtime.pcm_capacity_samples(),
        );
        runtime.set_pcm_soft_cap_samples(soft_cap);
        log::info!(
            "Audio device timing: cpu_hz={:.2}, synth_hz={}, cycles_per_sample={:.3}, target_latency_cycles={:.1} (~{} samples), soft_cap={} samples",
            timing.cpu_hz,
            timing.synth_sample_rate,
            timing.cpu_cycles_per_sample,
            timing.target_latency_cycles,
            timing.target_latency_samples,
            soft_cap
        );

        Self::from_runtime_and_engine(runtime, control_queues, engine)
    }

    /// Creates a device from prebuilt runtime and engine components.
    pub(crate) fn from_runtime_and_engine(
        runtime: SharedAudioDataPlane,
        control_queues: SharedAudioControlPlane,
        engine: AudioEngine,
    ) -> Self {
        Self {
            registers: [0; AUDIO_REGISTER_COUNT as usize],
            runtime,
            control_queues,
            engine,
            last_cycle: 0,
            last_osc3: 0,
            last_env3: 0,
        }
    }

    /// Returns a clone of the shared runtime handle.
    pub fn shared_data_plane(&self) -> SharedAudioDataPlane {
        self.runtime.clone()
    }

    /// Returns the synth sample rate produced by this device's engine.
    pub(crate) fn synth_sample_rate(&self) -> u32 {
        self.engine.synth_sample_rate()
    }

    /// Queues a register write tagged with the most recent CPU cycle.
    fn queue_write_event(&mut self, register: u8, value: u8) {
        log::trace!("Audio MMIO write: reg 0x{:02X} = 0x{:02X} @ cycle {}", register, value, self.last_cycle);
        self.control_queues.write_events.push_drop_oldest(AudioEvent {
            cycle: self.last_cycle,
            register,
            value,
        });
    }

    /// Resolves a readback value at the current CPU cycle.
    fn resolve_readback(&mut self, register: u8) -> u8 {
        let value = self
            .engine
            .resolve_readback_value(self.last_cycle, register, self.last_cycle);
        match register {
            r if r == AudioRegister::Osc3Read.as_u8() => self.last_osc3 = value,
            r if r == AudioRegister::Env3Read.as_u8() => self.last_env3 = value,
            _ => {}
        }
        value
    }
}

impl Memory for AudioMmioDevice {
    fn read_u8(&mut self, address: u16) -> u8 {
        if address >= AUDIO_REGISTER_COUNT {
            return 0;
        }

        let register = address as u8;
        if register == AudioRegister::Osc3Read.as_u8() || register == AudioRegister::Env3Read.as_u8() {
            let value = self.resolve_readback(register);
            self.registers[register as usize] = value;
            return value;
        }

        self.registers[address as usize]
    }

    fn write_u8(&mut self, address: u16, value: u8) {
        if address >= AUDIO_REGISTER_COUNT {
            return;
        }

        let register = address as u8;
        // Reject writes to read-only registers
        if register == AudioRegister::Osc3Read.as_u8() || register == AudioRegister::Env3Read.as_u8() {
            return;
        }

        self.registers[address as usize] = value;
        self.queue_write_event(register, value);
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        self.last_cycle = cycle;
        self.engine.advance_to_cpu_cycle(cycle);
        Interrupt::none()
    }
}
