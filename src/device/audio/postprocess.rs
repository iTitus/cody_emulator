//! Output-side postprocessing for synthesized audio.
//!
//! This stage pulls mono synth PCM from the runtime, handles simple underrun/
//! overrun policies, resamples to host rate, applies effects, and fans out to
//! the configured channel count.

use crate::device::audio::engine::SharedAudioDataPlane;
pub use crate::device::audio::fx::{
    AudioEffect, DcBlockEffect, GainEffect, OnePoleHighPassEffect, OnePoleLowPassEffect,
    SoftClipEffect,
};
use crate::device::audio::post_buffer_policy::{BufferPolicyManager, BufferState};
use crate::device::audio::post_resampler::LinearResampler;
use crate::device::audio::synth::SidLikeSynth;
use std::collections::VecDeque;
use std::time::Duration;

/// Runtime postprocessing and output format configuration.
#[derive(Debug, Clone)]
pub struct AudioPostProcessConfig {
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub preferred_output_buffer_frames: u32,
}

impl Default for AudioPostProcessConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            channels: 2,
            preferred_output_buffer_frames: 256,
        }
    }
}

/// Cloneable effect chain configuration for the postprocessor.
#[derive(Clone, Default)]
pub struct AudioEffectChain {
    pub pre: Vec<Box<dyn AudioEffect>>,
    pub post: Vec<Box<dyn AudioEffect>>,
}

/// Resampling and effects pipeline for postprocessed audio.
struct PostPipeline {
    resampler: LinearResampler,
    effects: AudioEffectChain,
}

impl PostPipeline {
    /// Creates a new pipeline with a linear resampler and empty effect chains.
    fn new() -> Self {
        Self {
            resampler: LinearResampler::new(),
            effects: AudioEffectChain::default(),
        }
    }

    /// Returns the last sample emitted by the resampler.
    fn last_sample(&self) -> f32 {
        self.resampler.last_sample()
    }

    /// Replaces the pre-resample effects.
    fn set_pre_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.effects.pre = effects;
    }

    /// Replaces the post-resample effects.
    fn set_post_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.effects.post = effects;
    }

    /// Returns a clone of the current effect chain configuration.
    fn effect_chain(&self) -> AudioEffectChain {
        self.effects.clone()
    }

    /// Computes how many source samples are needed for an output frame count.
    fn required_source_len(
        &self,
        frames: usize,
        synth_sample_rate: u32,
        output_sample_rate: u32,
    ) -> usize {
        self.resampler
            .required_source_len(frames, synth_sample_rate, output_sample_rate)
    }

    /// Renders interleaved output frames from the source FIFO.
    fn render_interleaved(
        &mut self,
        source_fifo: &mut VecDeque<f32>,
        out: &mut [f32],
        channels: usize,
        synth_sample_rate: u32,
        output_sample_rate: u32,
    ) -> usize {
        self.resampler.render_interleaved(
            source_fifo,
            out,
            channels,
            synth_sample_rate,
            output_sample_rate,
        )
    }

    /// Applies pre-resample effects in place.
    fn apply_pre_fx_chain(&mut self, chunk: &mut Vec<f32>, synth_sample_rate: u32) {
        if self.effects.pre.is_empty() {
            return;
        }
        for effect in self.effects.pre.iter_mut() {
            effect.prepare(1, synth_sample_rate);
            effect.process_in_place(chunk);
        }
    }

    /// Applies post-resample effects to the filled output range.
    fn apply_post_fx_chain(
        &mut self,
        out: &mut [f32],
        filled: usize,
        channels: usize,
        output_rate: u32,
    ) {
        if self.effects.post.is_empty() {
            return;
        }
        for effect in self.effects.post.iter_mut() {
            effect.prepare(channels, output_rate);
            effect.process_in_place(&mut out[..filled]);
        }
    }
}

/// Buffer controller that manages underruns, crossfades, and refill policy.
struct SourceBuffer {
    source_fifo: VecDeque<f32>,
    refill_chunk: Vec<f32>,
    fallback_synth_snapshot: Option<SidLikeSynth>,
    fallback_snapshot_updates: u64,
    buffer_policy: BufferPolicyManager,
}

impl SourceBuffer {
    /// Creates a new source buffer with default FIFO and policy settings.
    fn new() -> Self {
        Self {
            source_fifo: VecDeque::with_capacity(4096),
            refill_chunk: Vec::with_capacity(1024),
            fallback_synth_snapshot: None,
            fallback_snapshot_updates: 0,
            buffer_policy: BufferPolicyManager::new(),
        }
    }

    /// Enables or disables the initial catch-up sensitivity heuristic.
    fn set_initial_catchup_enabled(&mut self, enabled: bool) {
        self.buffer_policy.set_initial_catchup_enabled(enabled);
    }

    /// Returns whether initial catch-up sensitivity is enabled.
    fn initial_catchup_enabled(&self) -> bool {
        self.buffer_policy.initial_catchup_enabled()
    }

    /// Returns the current buffer management state.
    fn buffer_state(&self) -> BufferState {
        self.buffer_policy.buffer_state()
    }

    /// Predicts the next buffer state without consuming PCM.
    fn preview_buffer_state(
        &mut self,
        runtime: &SharedAudioDataPlane,
        synth_sample_rate: u32,
        wanted_len: usize,
    ) -> BufferState {
        self.buffer_policy
            .preview_buffer_state(runtime, synth_sample_rate, wanted_len)
    }

    /// Refills the FIFO with runtime PCM and applies policy/crossfade actions.
    fn refill_source_fifo(
        &mut self,
        runtime: &SharedAudioDataPlane,
        synth_sample_rate: u32,
        wanted_len: usize,
        pipeline: &mut PostPipeline,
    ) {
        if self.source_fifo.len() >= wanted_len {
            return;
        }

        // Calculate how many synth-rate samples we need to satisfy this callback.
        let missing = wanted_len - self.source_fifo.len();
        log::trace!(
            "Audio postprocess: refilling FIFO, missing {} samples (have {}, want {})",
            missing,
            self.source_fifo.len(),
            wanted_len
        );
        let mut chunk = std::mem::take(&mut self.refill_chunk);
        chunk.clear();
        let crossfade_len =
            self.buffer_policy
                .update_buffer_state(runtime, synth_sample_rate, wanted_len, missing);

        // Pull PCM from the runtime. If there is not enough, fall back to a snapshot.
        runtime.pop_pcm_samples(missing, &mut chunk);
        let snapshot = if chunk.len() < missing && self.fallback_synth_snapshot.is_none() {
            runtime.snapshot_clone()
        } else {
            None
        };
        self.handle_underrun_fallback(
            runtime,
            synth_sample_rate,
            missing,
            &mut chunk,
            snapshot,
            pipeline.last_sample(),
        );
        self.apply_crossfade_skip(runtime, crossfade_len, &mut chunk, pipeline.last_sample());
        pipeline.apply_pre_fx_chain(&mut chunk, synth_sample_rate);

        self.source_fifo.extend(chunk.drain(..));
        self.refill_chunk = chunk;
    }

    /// Fills missing samples using a synth snapshot or last-sample hold.
    fn handle_underrun_fallback(
        &mut self,
        runtime: &SharedAudioDataPlane,
        synth_sample_rate: u32,
        missing: usize,
        chunk: &mut Vec<f32>,
        snapshot: Option<SidLikeSynth>,
        last_sample: f32,
    ) {
        if chunk.len() < missing {
            let needed = missing - chunk.len();
            if self.fallback_synth_snapshot.is_none() {
                self.fallback_synth_snapshot = snapshot;
            }
            let next_underrun = if self.fallback_synth_snapshot.is_some() {
                BufferState::UnderrunShadow
            } else {
                BufferState::UnderrunHold
            };
            if self.buffer_policy.buffer_state() != next_underrun {
                match next_underrun {
                    BufferState::UnderrunShadow => {
                        self.fallback_snapshot_updates = runtime.snapshot_update_count();
                        log::warn!(
                            "Audio postprocess: UNDERRUN! Using shadow synth snapshot (updates={})",
                            runtime.snapshot_update_count()
                        );
                    }
                    BufferState::UnderrunHold => {
                        log::warn!(
                            "Audio postprocess: UNDERRUN! No shadow synth snapshot available, holding last sample"
                        );
                    }
                    _ => {}
                }
                self.buffer_policy.set_buffer_state(next_underrun);
            }
            if self.buffer_policy.buffer_state() == BufferState::UnderrunShadow {
                let current_updates = runtime.snapshot_update_count();
                if current_updates != self.fallback_snapshot_updates {
                    self.fallback_snapshot_updates = current_updates;
                    self.fallback_synth_snapshot = runtime.snapshot_clone();
                    log::debug!(
                        "Audio postprocess: refreshed shadow snapshot (updates={})",
                        current_updates
                    );
                }
            }
            if let Some(shadow) = self.fallback_synth_snapshot.as_mut() {
                log::trace!("Audio postprocess: rendering {} shadow samples", needed);
                for _ in 0..needed {
                    chunk.push(shadow.render_sample(synth_sample_rate));
                }
            } else {
                log::trace!(
                    "Audio postprocess: padding {} samples with last-sample hold",
                    needed
                );
                let hold = chunk.last().copied().unwrap_or(last_sample);
                chunk.resize(missing, hold);
            }
        } else if matches!(
            self.buffer_policy.buffer_state(),
            BufferState::UnderrunShadow | BufferState::UnderrunHold
        ) {
            // Real PCM resumed; stop shadow rendering.
            log::debug!("Audio postprocess: PCM buffer recovered, resuming normal output");
            self.fallback_synth_snapshot = None;
            self.buffer_policy.set_buffer_state(BufferState::Normal);
        }
    }

    /// Smooths skip events by crossfading the FIFO tail into the new chunk head.
    fn apply_crossfade_skip(
        &mut self,
        runtime: &SharedAudioDataPlane,
        crossfade_len: usize,
        chunk: &mut Vec<f32>,
        last_sample: f32,
    ) {
        if crossfade_len == 0 || chunk.is_empty() {
            return;
        }
        let available_tail = self.source_fifo.len();
        let fade_len = crossfade_len.min(chunk.len()).min(available_tail);
        if fade_len > 0 {
            let tail_start = available_tail - fade_len;
            for i in 0..fade_len {
                let t = (i + 1) as f32 / fade_len as f32;
                let fade_out = 1.0 - t;
                let fade_in = t;
                if let Some(tail_sample) = self.source_fifo.get_mut(tail_start + i) {
                    let head_sample = chunk[i];
                    *tail_sample = *tail_sample * fade_out + head_sample * fade_in;
                }
            }
            chunk.drain(..fade_len);
            let mut remaining_skip = crossfade_len.saturating_sub(fade_len);
            if remaining_skip > 0 {
                let extra_drop = remaining_skip.min(chunk.len());
                if extra_drop > 0 {
                    chunk.drain(..extra_drop);
                    remaining_skip -= extra_drop;
                }
                for _ in 0..remaining_skip {
                    runtime.pop_pcm_front();
                }
            }
        } else {
            let fade_from = self.source_fifo.back().copied().unwrap_or(last_sample);
            let fade_len = crossfade_len.min(chunk.len());
            for (i, sample) in chunk.iter_mut().take(fade_len).enumerate() {
                let t = (i + 1) as f32 / fade_len as f32;
                *sample = fade_from * (1.0 - t) + *sample * t;
            }
            let mut remaining_skip = crossfade_len.saturating_sub(fade_len);
            if remaining_skip > 0 {
                let extra_drop = remaining_skip.min(chunk.len().saturating_sub(fade_len));
                if extra_drop > 0 {
                    chunk.drain(fade_len..fade_len + extra_drop);
                    remaining_skip -= extra_drop;
                }
                for _ in 0..remaining_skip {
                    runtime.pop_pcm_front();
                }
            }
        }
    }
}

/// Pull-based postprocessor for converting synth PCM into host output frames.
pub struct AudioPostProcessor {
    runtime: SharedAudioDataPlane,
    synth_sample_rate: u32,
    buffer: SourceBuffer,
    pipeline: PostPipeline,
}

impl AudioPostProcessor {
    /// Creates a postprocessor bound to a shared runtime and synth sample rate.
    pub fn new(runtime: SharedAudioDataPlane, synth_sample_rate: u32) -> Self {
        Self {
            runtime,
            synth_sample_rate: synth_sample_rate.max(1),
            buffer: SourceBuffer::new(),
            pipeline: PostPipeline::new(),
        }
    }

    /// Returns the current buffer management state.
    pub fn buffer_state(&self) -> BufferState {
        self.buffer.buffer_state()
    }

    /// Enables or disables the initial catch-up sensitivity heuristic.
    pub fn set_initial_catchup_enabled(&mut self, enabled: bool) {
        self.buffer.set_initial_catchup_enabled(enabled);
    }

    /// Returns whether initial catch-up sensitivity is enabled.
    pub fn initial_catchup_enabled(&self) -> bool {
        self.buffer.initial_catchup_enabled()
    }

    /// Returns the next buffer state for diagnostics/tests without consuming samples.
    pub fn preview_buffer_state(&mut self, wanted_len: usize) -> BufferState {
        self.buffer
            .preview_buffer_state(&self.runtime, self.synth_sample_rate, wanted_len)
    }

    /// Replaces the pre-resample effect chain.
    pub fn set_pre_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.pipeline.set_pre_effects(effects);
    }

    /// Returns a copy of the shared runtime backing this postprocessor.
    pub fn shared_data_plane(&self) -> SharedAudioDataPlane {
        self.runtime.clone()
    }

    /// Returns a clone of the configured effect chains.
    pub fn effect_chain(&self) -> AudioEffectChain {
        self.pipeline.effect_chain()
    }

    /// Returns the synth sample rate used by this postprocessor.
    pub const fn synth_sample_rate(&self) -> u32 {
        self.synth_sample_rate
    }

    /// Replaces the post-resample effect chain.
    pub fn set_post_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.pipeline.set_post_effects(effects);
    }

    /// Renders interleaved output samples into `out` using `config`.
    pub fn render_interleaved(&mut self, out: &mut [f32], config: &AudioPostProcessConfig) {
        if out.is_empty() {
            return;
        }

        let sample_rate_hz = config.sample_rate_hz.max(1);
        let channels = config.channels.max(1);
        let frames = out.len() / channels;
        if frames == 0 {
            return;
        }
        if out.len() % channels != 0 {
            log::warn!(
                "Audio postprocess: output buffer length {} not divisible by channels {}",
                out.len(),
                channels
            );
        }

        let priming_target = self.runtime.target_latency_samples().max(1);
        if self.runtime.pcm_len() < priming_target {
            out.fill(0.0);
            return;
        }

        let needed =
            self.pipeline
                .required_source_len(frames, self.synth_sample_rate, sample_rate_hz);
        self.buffer.refill_source_fifo(
            &self.runtime,
            self.synth_sample_rate,
            needed,
            &mut self.pipeline,
        );

        let filled = self.pipeline.render_interleaved(
            &mut self.buffer.source_fifo,
            out,
            channels,
            self.synth_sample_rate,
            sample_rate_hz,
        );

        self.pipeline
            .apply_post_fx_chain(out, filled, channels, sample_rate_hz);

        if filled < out.len() {
            out[filled..].fill(0.0);
        }
    }

    /// Logs buffer stats when enabled or forced.
    pub fn print_debug_stuff(&mut self, force: bool) {
        if force || self.runtime.should_log_postprocess(Duration::from_secs(1)) {
            let pcm_len = self.runtime.pcm_len();
            let total = self.runtime.total_samples_produced();
            let underruns = self.runtime.underrun_samples();
            let snapshots = self.runtime.snapshot_update_count();
            log::debug!(
                "Audio postprocess: pcm_len={}, total_samples={}, underruns={}, snapshots={}, buffer_state={:?}",
                pcm_len,
                total,
                underruns,
                snapshots,
                self.buffer.buffer_state()
            );
        }
    }
}
