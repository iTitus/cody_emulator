//! MMIO facade for the SID-like audio block.
//!
//! The CPU interacts with it through the memory bus at `$D400`.
//! Writes are queued as cycle-stamped events and reads of special readback
//! registers are resolved through the audio engine.

use crate::device::audio::AudioConfig;
use crate::device::audio::core::AudioCore;
use crate::device::audio::engine::{AudioEvent, SharedAudioDataPlane};
use crate::device::audio::factory::{
    create_audio_mmio_device_default, create_audio_mmio_device_with_timing,
};
use crate::device::audio::registers::AudioRegister;
use crate::interrupt::Interrupt;
use crate::memory::Memory;

/// Base MMIO address for the audio register block.
pub const AUDIO_BASE: u16 = 0xD400;
/// Number of exposed MMIO registers in the block.
pub const AUDIO_REGISTER_COUNT: u16 = 0x20;

/// Memory-mapped audio device bridging CPU bus traffic and audio runtime.
#[derive(Debug, Clone)]
pub struct AudioMmioDevice {
    registers: [u8; AUDIO_REGISTER_COUNT as usize],
    core: AudioCore,
    last_cycle: usize,
    last_osc3: u8,
    last_env3: u8,
}

impl AudioMmioDevice {
    /// Creates an MMIO audio device using default timing parameters.
    pub fn new() -> Self {
        create_audio_mmio_device_default()
    }

    /// Creates an MMIO audio device with explicit timing parameters.
    pub fn with_timing(config: AudioConfig) -> Self {
        create_audio_mmio_device_with_timing(config)
    }

    /// Creates a device from a prebuilt audio core.
    pub(crate) fn from_core(core: AudioCore) -> Self {
        Self {
            registers: [0; AUDIO_REGISTER_COUNT as usize],
            core,
            last_cycle: 0,
            last_osc3: 0,
            last_env3: 0,
        }
    }

    /// Returns a clone of the shared runtime handle.
    pub fn shared_data_plane(&self) -> SharedAudioDataPlane {
        self.core.runtime()
    }

    /// Returns the synth sample rate produced by this device's engine.
    pub(crate) fn synth_sample_rate(&self) -> u32 {
        self.core.synth_sample_rate()
    }

    /// Updates CPU timing to keep audio in sync with the emulator clock.
    pub fn update_cpu_hz(&mut self, cpu_hz: f64) {
        self.core.engine_mut().update_cpu_hz(cpu_hz);
    }

    /// Queues a register write tagged with the most recent CPU cycle.
    fn queue_write_event(&mut self, register: u8, value: u8) {
        log::trace!(
            "Audio MMIO write: reg 0x{:02X} = 0x{:02X} @ cycle {}",
            register,
            value,
            self.last_cycle
        );
        self.core
            .control_plane()
            .write_events
            .push_drop_oldest(AudioEvent {
                cycle: self.last_cycle,
                register,
                value,
            });
    }

    /// Resolves a readback value at the current CPU cycle and caches it.
    fn resolve_readback(&mut self, register: u8) -> u8 {
        let value = self.core.engine_mut().resolve_readback_value(
            self.last_cycle,
            register,
            self.last_cycle,
        );
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
        if register == AudioRegister::Osc3Read.as_u8()
            || register == AudioRegister::Env3Read.as_u8()
        {
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
        if register == AudioRegister::Osc3Read.as_u8()
            || register == AudioRegister::Env3Read.as_u8()
        {
            return;
        }

        self.registers[address as usize] = value;
        self.queue_write_event(register, value);
    }

    fn update(&mut self, cycle: usize) -> Interrupt {
        self.last_cycle = cycle;
        self.core.engine_mut().advance_to_cpu_cycle(cycle);
        Interrupt::none()
    }
}
