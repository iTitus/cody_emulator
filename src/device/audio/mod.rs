//! SID-like audio subsystem for the Cody emulator.
//!
//! This module mirrors the Cody Computer's audio register map documented in
//! `cody-computer/Spin/cody_audio.spin`:
//! - Base address: `$D400`
//! - Readback registers: `$D41B` (OSC3), `$D41C` (ENV3)

pub mod core;
pub mod engine;
pub mod factory;
pub mod fx;
pub mod host;
pub mod mmiodev;
pub mod post_buffer_policy;
pub mod post_resampler;
pub mod postprocess;
pub mod queue;
pub mod registers;
pub mod source;
pub mod synth;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub cpu_hz: f64,
    pub synth_sample_rate: u32,
    pub target_latency_cycles: f64,
}

impl AudioConfig {
    pub fn new(cpu_hz: f64, synth_sample_rate: u32, target_latency_cycles: f64) -> Self {
        Self {
            cpu_hz,
            synth_sample_rate,
            target_latency_cycles,
        }
    }
}

pub(crate) fn compute_soft_cap_samples(
    target_latency_samples: usize,
    staging_capacity_samples: usize,
) -> usize {
    let min_cap = target_latency_samples.saturating_mul(2);
    let max_cap = staging_capacity_samples.saturating_sub(target_latency_samples);
    let desired_cap = 1024.min(max_cap);
    if max_cap < min_cap {
        return max_cap.max(1);
    }

    desired_cap.clamp(min_cap, max_cap)
}

pub use core::AudioCore;
pub use factory::{
    AudioPipeline, create_audio_core, create_audio_pipeline, create_audio_pipeline_with_timing,
};
pub use host::CpalHost;
pub use mmiodev::{AUDIO_BASE, AUDIO_REGISTER_COUNT, AudioMmioDevice};
pub use postprocess::{AudioPostProcessConfig, AudioPostProcessor};
pub use registers::AudioRegister;
