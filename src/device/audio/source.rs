//! Shared PCM source abstraction for postprocessing and buffer policy.

use crate::device::audio::engine::SharedAudioDataPlane;
use crate::device::audio::synth::SidLikeSynth;

/// Source of mono PCM samples and runtime metadata.
pub trait PcmSource: Send + Sync {
    fn pop_pcm_samples(&self, wanted: usize, out: &mut Vec<f32>);
    fn pop_pcm_front(&self);
    fn pcm_len(&self) -> usize;
    fn pcm_soft_cap_samples(&self) -> usize;
    fn target_latency_samples(&self) -> usize;
    fn callback_effective_synth_samples(&self) -> usize;
    fn catchup_trigger_strictness_q10(&self) -> u32;
    fn snapshot_clone(&self) -> Option<SidLikeSynth>;
    fn snapshot_update_count(&self) -> u64;
    fn total_samples_produced(&self) -> u64;
    fn underrun_samples(&self) -> u64;
}

impl PcmSource for SharedAudioDataPlane {
    fn pop_pcm_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        self.as_ref().pop_pcm_samples(wanted, out);
    }

    fn pop_pcm_front(&self) {
        self.as_ref().pop_pcm_front();
    }

    fn pcm_len(&self) -> usize {
        self.as_ref().pcm_len()
    }

    fn pcm_soft_cap_samples(&self) -> usize {
        self.as_ref().pcm_soft_cap_samples()
    }

    fn target_latency_samples(&self) -> usize {
        self.as_ref().target_latency_samples()
    }

    fn callback_effective_synth_samples(&self) -> usize {
        self.as_ref().callback_effective_synth_samples()
    }

    fn catchup_trigger_strictness_q10(&self) -> u32 {
        self.as_ref().catchup_trigger_strictness_q10()
    }

    fn snapshot_clone(&self) -> Option<SidLikeSynth> {
        self.as_ref().snapshot_clone()
    }

    fn snapshot_update_count(&self) -> u64 {
        self.as_ref().snapshot_update_count()
    }

    fn total_samples_produced(&self) -> u64 {
        self.as_ref().total_samples_produced()
    }

    fn underrun_samples(&self) -> u64 {
        self.as_ref().underrun_samples()
    }
}
