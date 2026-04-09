//! Construction helpers for the audio subsystem graph.

use crate::device::audio::{AudioConfig, PcmQueueKind};
use crate::device::audio::core::AudioCore;
use crate::device::audio::engine::{
    AudioControlPlane, AudioDataPlane, AudioEngine, SharedAudioControlPlane,
    SharedAudioDataPlane,
};
use crate::device::audio::fx::{GainEffect, OnePoleHighPassEffect, SoftClipEffect};
use crate::device::audio::host::{CpalHost, StubHost};
use crate::device::audio::mmiodev::AudioMmioDevice;
use crate::device::audio::postprocess::{AudioPostProcessConfig, AudioPostProcessor};
use crate::device::audio::queue::{new_dummy_pcm_buffer, new_pcm_buffer};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_PCM_CAPACITY: usize = 4096;
const DEFAULT_WRITE_QUEUE_CAPACITY: usize = 8192;
const DEFAULT_CPU_HZ: f64 = 1_000_000.0;
const DEFAULT_SYNTH_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_TARGET_LATENCY_SAMPLES: u32 = 128;

/// High-level audio pipeline bundle used by the frontend.
pub struct AudioPipeline {
    pub mmio: AudioMmioDevice,
    pub post: AudioPostProcessor,
}

/// Frontend-facing audio startup options.
#[derive(Debug, Clone, Copy)]
pub struct FrontendAudioOptions {
    pub audio_off: bool,
    pub audio_no_initial_catchup: bool,
    pub audio_buffer_frames: u32,
}

/// Frontend-facing audio host wrapper.
pub enum FrontendAudioHost {
    Backend(CpalHost),
    Stub(StubHost),
}

impl FrontendAudioHost {
    pub fn wait_ready(&mut self, timeout: Duration) -> bool {
        match self {
            Self::Backend(host) => host.wait_ready(timeout),
            Self::Stub(host) => host.wait_ready(timeout),
        }
    }
}

/// Fully assembled frontend audio bundle.
pub struct FrontendAudio {
    pub mmio: AudioMmioDevice,
    pub host: FrontendAudioHost,
}

fn default_audio_config() -> AudioConfig {
    let cycles_per_sample = DEFAULT_CPU_HZ / DEFAULT_SYNTH_SAMPLE_RATE as f64;
    let target_latency_cycles = cycles_per_sample * DEFAULT_TARGET_LATENCY_SAMPLES as f64;
    AudioConfig::new(
        DEFAULT_CPU_HZ,
        DEFAULT_SYNTH_SAMPLE_RATE,
        target_latency_cycles,
    )
}

fn apply_default_effects(post: &mut AudioPostProcessor) {
    post.set_pre_effects(vec![
        Box::new(GainEffect::new(1.0)),
        Box::new(OnePoleHighPassEffect::new(40.0)),
        Box::new(SoftClipEffect),
    ]);
    post.set_post_effects(vec![Box::new(GainEffect::new(1.0)), Box::new(SoftClipEffect)]);
}

/// Creates an MMIO audio device using the default timing/profile.
pub fn create_audio_mmio_device_default() -> AudioMmioDevice {
    create_audio_mmio_device_with_timing(default_audio_config())
}

/// Creates an MMIO audio device with explicit timing/profile.
pub fn create_audio_mmio_device_with_timing(config: AudioConfig) -> AudioMmioDevice {
    let core = create_audio_core(config, DEFAULT_PCM_CAPACITY, DEFAULT_WRITE_QUEUE_CAPACITY);
    AudioMmioDevice::from_core(core)
}

/// Creates the shared data plane used by the engine and postprocessor.
pub fn create_audio_data_plane(pcm_capacity: usize) -> SharedAudioDataPlane {
    create_audio_data_plane_with_queue_kind(pcm_capacity, PcmQueueKind::Real)
}

/// Creates the shared data plane used by the engine and postprocessor.
pub fn create_audio_data_plane_with_queue_kind(
    pcm_capacity: usize,
    pcm_queue_kind: PcmQueueKind,
) -> SharedAudioDataPlane {
    let pcm = match pcm_queue_kind {
        PcmQueueKind::Real => new_pcm_buffer(pcm_capacity),
        PcmQueueKind::Dummy => new_dummy_pcm_buffer(pcm_capacity),
    };
    Arc::new(AudioDataPlane::new(pcm))
}

/// Creates lock-free control queues shared between MMIO and engine.
pub fn create_audio_control_plane(write_queue_capacity: usize) -> SharedAudioControlPlane {
    Arc::new(AudioControlPlane::new(write_queue_capacity))
}

/// Creates an audio engine bound to an existing shared runtime.
pub fn create_audio_engine(
    runtime: SharedAudioDataPlane,
    control_queues: SharedAudioControlPlane,
    config: AudioConfig,
) -> AudioEngine {
    AudioEngine::new(runtime, control_queues, config)
}

/// Creates a bundled audio core with shared runtime, control plane, and engine.
pub fn create_audio_core(
    config: AudioConfig,
    pcm_capacity: usize,
    write_queue_capacity: usize,
) -> AudioCore {
    create_audio_core_with_queue_kind(
        config,
        pcm_capacity,
        write_queue_capacity,
        PcmQueueKind::Real,
    )
}

/// Creates a bundled audio core with shared runtime, control plane, and engine.
pub fn create_audio_core_with_queue_kind(
    config: AudioConfig,
    pcm_capacity: usize,
    write_queue_capacity: usize,
    pcm_queue_kind: PcmQueueKind,
) -> AudioCore {
    let pcm = match pcm_queue_kind {
        PcmQueueKind::Real => new_pcm_buffer(pcm_capacity),
        PcmQueueKind::Dummy => new_dummy_pcm_buffer(pcm_capacity),
    };
    AudioCore::new(config, pcm, write_queue_capacity)
}

/// Builds the default audio pipeline used by the frontend.
pub fn create_audio_pipeline(audio_off: bool) -> AudioPipeline {
    let pcm_queue_kind = if audio_off {
        PcmQueueKind::Dummy
    } else {
        PcmQueueKind::Real
    };
    let config = default_audio_config();
    let core = create_audio_core_with_queue_kind(
        config,
        DEFAULT_PCM_CAPACITY,
        DEFAULT_WRITE_QUEUE_CAPACITY,
        pcm_queue_kind,
    );
    let mmio = AudioMmioDevice::from_core(core);
    let mut post = AudioPostProcessor::new(mmio.shared_data_plane(), mmio.synth_sample_rate());
    apply_default_effects(&mut post);
    AudioPipeline { mmio, post }
}

/// Builds full frontend audio (MMIO + host) from startup options.
pub fn create_frontend_audio(options: FrontendAudioOptions) -> FrontendAudio {
    let mut pipeline = create_audio_pipeline(options.audio_off);
    pipeline
        .post
        .set_initial_catchup_enabled(!options.audio_no_initial_catchup);

    let host = if options.audio_off {
        log::info!(
            "Audio backend disabled (--audio-off): using stub host and bypassing postprocess/resampling"
        );
        FrontendAudioHost::Stub(StubHost::new(pipeline.post))
    } else {
        FrontendAudioHost::Backend(CpalHost::new(
            pipeline.post,
            AudioPostProcessConfig {
                preferred_output_buffer_frames: options.audio_buffer_frames,
                ..AudioPostProcessConfig::default()
            },
        ))
    };

    FrontendAudio {
        mmio: pipeline.mmio,
        host,
    }
}

/// Builds an audio pipeline with explicit timing parameters.
pub fn create_audio_pipeline_with_timing(
    cpu_hz: f64,
    synth_sample_rate: u32,
    target_latency_cycles: f64,
) -> AudioPipeline {
    create_audio_pipeline_with_timing_and_queue_kind(
        cpu_hz,
        synth_sample_rate,
        target_latency_cycles,
        PcmQueueKind::Real,
    )
}

/// Builds an audio pipeline with explicit timing parameters and queue kind.
pub fn create_audio_pipeline_with_timing_and_queue_kind(
    cpu_hz: f64,
    synth_sample_rate: u32,
    target_latency_cycles: f64,
    pcm_queue_kind: PcmQueueKind,
) -> AudioPipeline {
    let config = AudioConfig::new(cpu_hz, synth_sample_rate, target_latency_cycles);
    let core = create_audio_core_with_queue_kind(
        config,
        DEFAULT_PCM_CAPACITY,
        DEFAULT_WRITE_QUEUE_CAPACITY,
        pcm_queue_kind,
    );
    let mmio = AudioMmioDevice::from_core(core);
    let mut post = AudioPostProcessor::new(mmio.shared_data_plane(), mmio.synth_sample_rate());
    apply_default_effects(&mut post);
    AudioPipeline { mmio, post }
}
