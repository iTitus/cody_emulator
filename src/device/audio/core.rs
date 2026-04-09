//! Core audio runtime components shared across frontends.

use crate::device::audio::AudioConfig;
use crate::device::audio::engine::{
    AudioControlPlane, AudioDataPlane, AudioEngine, AudioTiming, SharedAudioControlPlane,
    SharedAudioDataPlane,
};
use std::sync::Arc;

/// Bundles the audio engine with its shared runtime and control plane.
#[derive(Debug, Clone)]
pub struct AudioCore {
    runtime: SharedAudioDataPlane,
    control_queues: SharedAudioControlPlane,
    engine: AudioEngine,
    timing: AudioTiming,
}

impl AudioCore {
    /// Creates a new audio core with explicit buffering capacities.
    pub fn new(config: AudioConfig, pcm_capacity: usize, write_queue_capacity: usize) -> Self {
        let runtime = Arc::new(AudioDataPlane::new(pcm_capacity));
        let control_queues = Arc::new(AudioControlPlane::new(write_queue_capacity));
        let engine = AudioEngine::new(runtime.clone(), control_queues.clone(), config);
        let timing = AudioTiming::new(config);
        runtime.configure_buffering(timing.target_latency_samples);

        Self {
            runtime,
            control_queues,
            engine,
            timing,
        }
    }

    /// Returns a clone of the shared runtime handle.
    pub fn runtime(&self) -> SharedAudioDataPlane {
        self.runtime.clone()
    }

    /// Returns a clone of the shared control plane.
    pub fn control_plane(&self) -> SharedAudioControlPlane {
        self.control_queues.clone()
    }

    /// Returns a mutable reference to the audio engine.
    pub fn engine_mut(&mut self) -> &mut AudioEngine {
        &mut self.engine
    }

    /// Returns the synth sample rate produced by this core.
    pub fn synth_sample_rate(&self) -> u32 {
        self.engine.synth_sample_rate()
    }

    /// Returns the derived audio timing for diagnostics.
    pub fn timing(&self) -> AudioTiming {
        self.timing
    }
}
