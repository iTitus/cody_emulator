//! Construction helpers for the audio subsystem graph.

use crate::device::audio::engine::{
    AudioControlPlane,
    AudioEngine,
    AudioDataPlane,
    SharedAudioControlPlane,
    SharedAudioDataPlane,
};
use crate::device::audio::core::AudioCore;
use crate::device::audio::AudioConfig;
use crate::device::audio::fx::{GainEffect, OnePoleHighPassEffect, SoftClipEffect};
use crate::device::audio::mmiodev::AudioMmioDevice;
use crate::device::audio::postprocess::AudioPostProcessor;
use std::sync::Arc;

/// High-level audio pipeline bundle used by the frontend.
pub struct AudioPipeline {
    pub mmio: AudioMmioDevice,
    pub post: AudioPostProcessor,
}

/// Creates the shared data plane used by the engine and postprocessor.
pub fn create_audio_data_plane(
    pcm_capacity: usize,
) -> SharedAudioDataPlane {
    Arc::new(AudioDataPlane::new(pcm_capacity))
}

/// Creates lock-free control queues shared between MMIO and engine.
pub fn create_audio_control_plane(
    write_queue_capacity: usize,
) -> SharedAudioControlPlane {
    Arc::new(AudioControlPlane::new(write_queue_capacity))
}

/// Creates an audio engine bound to an existing shared runtime.
pub fn create_audio_engine(
    runtime: SharedAudioDataPlane,
    control_queues: SharedAudioControlPlane,
    config: AudioConfig,
) -> AudioEngine {
    AudioEngine::new(
        runtime,
        control_queues,
        config,
    )
}

/// Creates a bundled audio core with shared runtime, control plane, and engine.
pub fn create_audio_core(
    config: AudioConfig,
    pcm_capacity: usize,
    write_queue_capacity: usize,
) -> AudioCore {
    AudioCore::new(config, pcm_capacity, write_queue_capacity)
}

/// Builds the default audio pipeline used by the frontend.
pub fn create_audio_pipeline() -> AudioPipeline {
    let mmio = AudioMmioDevice::new();
    let mut post = AudioPostProcessor::new(mmio.shared_data_plane(), mmio.synth_sample_rate());
    post.set_pre_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(OnePoleHighPassEffect::new(40.0)),
        Box::new(SoftClipEffect),
    ]);
    post.set_post_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
    ]);
    AudioPipeline { mmio, post }
}

/// Builds an audio pipeline with explicit timing parameters.
pub fn create_audio_pipeline_with_timing(
    cpu_hz: f64,
    synth_sample_rate: u32,
    target_latency_cycles: f64,
) -> AudioPipeline {
    let config = AudioConfig::new(cpu_hz, synth_sample_rate, target_latency_cycles);
    let mmio = AudioMmioDevice::with_timing(config);
    let mut post = AudioPostProcessor::new(mmio.shared_data_plane(), mmio.synth_sample_rate());
    post.set_pre_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(OnePoleHighPassEffect::new(40.0)),
        Box::new(SoftClipEffect),
    ]);
    post.set_post_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
    ]);
    AudioPipeline { mmio, post }
}
