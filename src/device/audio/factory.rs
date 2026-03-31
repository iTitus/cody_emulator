//! Construction helpers for the audio subsystem graph.

use crate::device::audio::engine::{
    AudioControlPlane,
    AudioEngine,
    AudioDataPlane,
    SharedAudioControlPlane,
    SharedAudioDataPlane,
};
use crate::device::audio::AudioConfig;
use crate::device::audio::fx::{GainEffect, OnePoleHighPassEffect, SoftClipEffect};
use crate::device::audio::mmiodev::AudioMmioDevice;
use crate::device::audio::postprocess::AudioPostProcessor;
use std::sync::Arc;

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

/// Builds the default audio pipeline used by the frontend.
pub fn create_audio_pipeline() -> (AudioMmioDevice, AudioPostProcessor) {
    let audio = AudioMmioDevice::new();
    let mut post = AudioPostProcessor::new(audio.shared_data_plane(), audio.synth_sample_rate());
    post.set_pre_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
        Box::new(OnePoleHighPassEffect::new(40.0)),
    ]);
    post.set_post_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
    ]);
    (audio, post)
}

/// Builds an audio pipeline with explicit timing parameters.
pub fn create_audio_pipeline_with_timing(
    cpu_hz: f64,
    synth_sample_rate: u32,
    target_latency_cycles: f64,
) -> (AudioMmioDevice, AudioPostProcessor) {
    let config = AudioConfig::new(cpu_hz, synth_sample_rate, target_latency_cycles);
    let audio = AudioMmioDevice::with_timing(config);
    let mut post = AudioPostProcessor::new(audio.shared_data_plane(), audio.synth_sample_rate());
    post.set_pre_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
        Box::new(OnePoleHighPassEffect::new(40.0)),
    ]);
    post.set_post_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(SoftClipEffect),
    ]);
    (audio, post)
}
