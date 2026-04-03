//! Output-side postprocessing for synthesized audio.
//!
//! This stage pulls mono synth PCM from the runtime, handles simple underrun/
//! overrun policies, resamples to host rate, applies effects, and fans out to
//! the configured channel count.

use crate::device::audio::engine::SharedAudioDataPlane;
pub use crate::device::audio::fx::{
    AudioEffect,
    DcBlockEffect,
    GainEffect,
    OnePoleHighPassEffect,
    OnePoleLowPassEffect,
    SoftClipEffect,
};
use crate::device::audio::synth::SidLikeSynth;
use std::collections::VecDeque;
use std::sync::Once;
use std::time::{Duration, Instant};

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


/// Unified buffer management state machine.
///
/// Each state determines how the buffer is managed and what actions are taken:
///
/// - Normal: Buffer is within target range; normal PCM flow, no catch-up or fallback.
/// - Catchup: Buffer is above target (but not overrun); old samples are gradually skipped to reduce latency, with crossfade smoothing.
/// - Overrun: Buffer is far above soft cap; aggressively drops old samples to avoid excessive latency.
/// - UnderrunShadow: Not enough PCM; output is synthesized from a shadow snapshot of the synth state (if available).
/// - UnderrunHold: Not enough PCM and no snapshot; output is held at the last sample (audio freeze).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    /// Buffer is healthy; normal PCM output, no special handling.
    Normal,
    /// Buffer is above target; gradually skip old samples to catch up, with crossfade.
    Catchup,
    /// Buffer is far above soft cap; drop old samples aggressively to avoid latency.
    Overrun,
    /// Buffer underrun; synth snapshot is used to generate fallback audio.
    UnderrunShadow,
    /// Buffer underrun and no snapshot; hold last sample (audio freeze).
    UnderrunHold,
}

#[derive(Debug, Clone, Copy)]
struct BufferBounds {
    target_latency_samples: usize,
    soft_cap_samples: usize,
    buffered_samples: usize,
    frame_samples: usize,
    drift_guard_samples: usize,
    over_soft_cap: bool,
    target_buffer_samples: usize,
    max_crossfade_samples: usize,
}

#[derive(Debug, Clone, Copy)]
struct BufferPolicy {
    bounds: BufferBounds,
    next_state: BufferState,
    one_shot_catchup_active: bool,
}

#[derive(Debug, Clone)]
struct PostProcessStats {
    stats_last: Instant,
    catchup_credit: f64,
    catchup_last: Instant,
    start_instant: Instant,
    one_shot_catchup_done: bool,
}


/// Pull-based postprocessor for converting synth PCM into host output frames.
pub struct AudioPostProcessor {
    runtime: SharedAudioDataPlane,
    synth_sample_rate: u32,
    source_fifo: VecDeque<f32>,
    refill_chunk: Vec<f32>,
    source_pos: f64,
    last_sample: f32,
    fallback_synth_snapshot: Option<SidLikeSynth>,
    fallback_snapshot_updates: u64,
    buffer_state: BufferState,
    pre_fx: Vec<Box<dyn AudioEffect>>,
    post_fx: Vec<Box<dyn AudioEffect>>,
    stats: PostProcessStats,
    // catchup_active: bool, // No longer needed; replaced by BufferState
}

impl AudioPostProcessor {
    /// Creates a postprocessor bound to a shared runtime and synth sample rate.
    pub fn new(runtime: SharedAudioDataPlane, synth_sample_rate: u32) -> Self {
        Self {
            runtime,
            synth_sample_rate: synth_sample_rate.max(1),
            source_fifo: VecDeque::with_capacity(4096),
            refill_chunk: Vec::with_capacity(1024),
            source_pos: 0.0,
            last_sample: 0.0,
            fallback_synth_snapshot: None,
            fallback_snapshot_updates: 0,
            buffer_state: BufferState::Normal,
            pre_fx: Vec::new(),
            post_fx: Vec::new(),
            stats: PostProcessStats {
                stats_last: Instant::now(),
                catchup_credit: 0.0,
                catchup_last: Instant::now(),
                start_instant: Instant::now(),
                one_shot_catchup_done: false,
            },
            // catchup_active: false, // Removed unused field
        }
    }

    /// Returns the current buffer management state.
    pub fn buffer_state(&self) -> BufferState {
        self.buffer_state
    }

    /// Returns the next buffer state for diagnostics/tests without consuming samples.
    pub fn preview_buffer_state(&mut self, wanted_len: usize) -> BufferState {
        let now = Instant::now();
        let policy = self.prepare_buffer_policy(wanted_len, now);
        policy.next_state
    }

    /// Replaces the pre-resample effect chain.
    pub fn set_pre_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.pre_fx = effects;
    }

    /// Returns a copy of the shared runtime backing this postprocessor.
    pub fn shared_data_plane(&self) -> SharedAudioDataPlane {
        self.runtime.clone()
    }

    /// Returns the synth sample rate used by this postprocessor.
    pub const fn synth_sample_rate(&self) -> u32 {
        self.synth_sample_rate
    }

    /// Replaces the post-resample effect chain.
    pub fn set_post_effects(&mut self, effects: Vec<Box<dyn AudioEffect>>) {
        self.post_fx = effects;
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

        let ratio = self.synth_sample_rate as f64 / sample_rate_hz as f64;
        let needed = (self.source_pos + frames as f64 * ratio).ceil() as usize + 2;
        self.refill_source_fifo(needed);

        let mut out_idx = 0usize;
        for _ in 0..frames {
            if self.source_fifo.len() < 2 {
                // Freeze output during underrun by holding the last emitted sample.
                self.source_fifo.push_back(self.last_sample);
                self.source_fifo.push_back(self.last_sample);
            }

            let idx0 = self.source_pos.floor() as usize;
            let frac = (self.source_pos - idx0 as f64) as f32;
            let s0 = self.source_fifo.get(idx0).copied().unwrap_or(0.0);
            let s1 = self.source_fifo.get(idx0 + 1).copied().unwrap_or(s0);
            let sample = s0 + (s1 - s0) * frac;

            self.last_sample = sample;

            for _ in 0..channels {
                if out_idx < out.len() {
                    out[out_idx] = sample;
                    out_idx += 1;
                }
            }

            self.source_pos += ratio;
            let consumed = self.source_pos.floor() as usize;
            if consumed > 0 {
                for _ in 0..consumed {
                    let _ = self.source_fifo.pop_front();
                }
                self.source_pos -= consumed as f64;
            }
        }

        let filled = out_idx.min(out.len());
        if !self.post_fx.is_empty() {
            for effect in self.post_fx.iter_mut() {
                effect.prepare(channels, sample_rate_hz);
                effect.process_in_place(&mut out[..filled]);
            }
        }

        if filled < out.len() {
            out[filled..].fill(0.0);
        }
    }

    pub fn print_debug_stuff(&mut self, force: bool) {
        if self.stats.stats_last.elapsed() >= Duration::from_secs(1) || force {
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
                self.buffer_state
            );
            self.stats.stats_last = Instant::now();
        }
    }

    /// Computes derived buffer thresholds and soft caps used by the state machine.
    fn compute_buffer_bounds(&self, wanted_len: usize) -> BufferBounds {
        let target_latency_samples = self.runtime.target_latency_samples();
        let mut soft_cap_samples = self.runtime.pcm_soft_cap_samples();
        if soft_cap_samples == 0 {
            soft_cap_samples = wanted_len.saturating_mul(4);
        } else {
            log::trace!("Audio postprocess: using soft cap {} samples", soft_cap_samples);
        }
        let buffered_samples = self.runtime.pcm_len();
        let frame_samples = self.synth_sample_rate.saturating_div(60).max(1) as usize;
        let drift_guard_samples = if target_latency_samples > 0 {
            let guard = target_latency_samples + frame_samples + frame_samples / 4;
            guard.max(wanted_len)
        } else {
            wanted_len
        };
        let over_soft_cap = buffered_samples > soft_cap_samples;
        let target_buffer_samples = if target_latency_samples > 0 {
            target_latency_samples.max(wanted_len)
        } else {
            wanted_len
        };
        let max_crossfade_samples = frame_samples.saturating_div(2).max(64);

        BufferBounds {
            target_latency_samples,
            soft_cap_samples,
            buffered_samples,
            frame_samples,
            drift_guard_samples,
            over_soft_cap,
            target_buffer_samples,
            max_crossfade_samples,
        }
    }

    /// Logs transitions between buffer states with current thresholds.
    fn log_state_change(&self, next_state: BufferState, bounds: &BufferBounds) {
        if next_state != self.buffer_state {
            static BUFFER_STATE_NOTICE: Once = Once::new();
            BUFFER_STATE_NOTICE.call_once(|| {
                log::warn!(
                    "Audio postprocess: BUFFER STATE changes indicate buffer management actions; seeing one early during emulation is normal. The audio engine tries to reduce latency early that way."
                );
            });
            log::warn!(
                "Audio postprocess: BUFFER STATE CHANGE {:?} -> {:?} (buffered={}, target_latency={}, target_buffer={}, drift_guard={}, soft_cap={})",
                self.buffer_state,
                next_state,
                bounds.buffered_samples,
                bounds.target_latency_samples,
                bounds.target_buffer_samples,
                bounds.drift_guard_samples,
                bounds.soft_cap_samples
            );
        }
    }

    /// Selects the next buffer state and whether the one-shot catchup is active.
    fn select_buffer_state(&mut self, bounds: &BufferBounds, now: Instant) -> (BufferState, bool) {
        let startup_ready = now.saturating_duration_since(self.stats.start_instant) >= Duration::from_secs(1);
        if startup_ready && !self.stats.one_shot_catchup_done && bounds.buffered_samples <= bounds.target_buffer_samples {
            self.stats.one_shot_catchup_done = true;
        }
        let one_shot_catchup_active =
            startup_ready && !self.stats.one_shot_catchup_done && bounds.buffered_samples > bounds.target_buffer_samples;
        let catchup_requested = if self.buffer_state == BufferState::Catchup {
            bounds.buffered_samples > bounds.target_buffer_samples
        } else {
            (bounds.buffered_samples > bounds.drift_guard_samples && !bounds.over_soft_cap)
                || one_shot_catchup_active
        };

        let next_state = if bounds.over_soft_cap {
            BufferState::Overrun
        } else if catchup_requested {
            BufferState::Catchup
        } else {
            BufferState::Normal
        };

        self.log_state_change(next_state, bounds);

        (next_state, one_shot_catchup_active)
    }

    /// Prepares buffer bounds and selects the next state for the current refill.
    fn prepare_buffer_policy(&mut self, wanted_len: usize, now: Instant) -> BufferPolicy {
        let bounds = self.compute_buffer_bounds(wanted_len);
        let (next_state, one_shot_catchup_active) = self.select_buffer_state(&bounds, now);
        BufferPolicy {
            bounds,
            next_state,
            one_shot_catchup_active,
        }
    }

    /// Logs refill bounds and state transitions in a consistent format.
    fn log_refill_bounds(
        &self,
        bounds: &BufferBounds,
        next_state: BufferState,
        wanted_len: usize,
        missing: usize,
    ) {
        log::trace!(
            "Audio postprocess: refill bounds buffered={}, target_latency={}, target_buffer={}, drift_guard={}, soft_cap={}, state={:?} -> {:?}, want={}, missing={}",
            bounds.buffered_samples,
            bounds.target_latency_samples,
            bounds.target_buffer_samples,
            bounds.drift_guard_samples,
            bounds.soft_cap_samples,
            self.buffer_state,
            next_state,
            wanted_len,
            missing
        );
    }

    /// Applies the selected buffer policy and returns the crossfade length if skipping.
    fn apply_buffer_state(
        &mut self,
        next_state: BufferState,
        bounds: &BufferBounds,
        catchup_elapsed: Duration,
        one_shot_catchup_active: bool,
    ) -> usize {
        let mut crossfade_len = 0usize;

        match next_state {
            BufferState::Catchup => {
                let excess = bounds
                    .buffered_samples
                    .saturating_sub(bounds.target_buffer_samples);
                let catchup_secs = 0.5_f64; // More aggressive catchup
                let rate = excess as f64 / catchup_secs;
                self.stats.catchup_credit += rate * catchup_elapsed.as_secs_f64();
                let mut max_skip = bounds.frame_samples.saturating_div(4).max(8);
                if excess > bounds.frame_samples * 2 {
                    max_skip = excess;
                }
                let mut to_skip = self.stats.catchup_credit.floor() as usize;
                if to_skip > max_skip {
                    to_skip = max_skip;
                }
                if to_skip > excess {
                    to_skip = excess;
                }
                if to_skip > 0 {
                    let planned_fade = to_skip.min(bounds.max_crossfade_samples);
                    let pop_skip = to_skip.saturating_sub(planned_fade);
                    for _ in 0..pop_skip {
                        self.runtime.pop_pcm_front();
                    }
                    self.stats.catchup_credit -= to_skip as f64;
                    crossfade_len = crossfade_len.max(planned_fade);
                    let post_skip = self.runtime.pcm_len();
                    if one_shot_catchup_active && post_skip <= bounds.target_buffer_samples {
                        self.stats.one_shot_catchup_done = true;
                    }
                }
                let post_skip = self.runtime.pcm_len();
                if post_skip > bounds.soft_cap_samples {
                    let clamp = post_skip - bounds.soft_cap_samples;
                    log::warn!("Audio postprocess: CLAMP after catchup, dropping {} samples", clamp);
                    for _ in 0..clamp {
                        self.runtime.pop_pcm_front();
                    }
                }
            }
            BufferState::Overrun => {
                let to_skip = bounds
                    .buffered_samples
                    .saturating_sub(bounds.target_buffer_samples);
                log::warn!(
                    "Audio postprocess: OVERRUN detected! Buffered={}, max={}, skipping {} samples",
                    bounds.buffered_samples,
                    bounds.soft_cap_samples,
                    to_skip
                );
                if to_skip > 0 {
                    let planned_fade = to_skip.min(bounds.max_crossfade_samples);
                    let pop_skip = to_skip.saturating_sub(planned_fade);
                    crossfade_len = crossfade_len.max(planned_fade);
                    for _ in 0..pop_skip {
                        self.runtime.pop_pcm_front();
                    }
                }
                let post_skip = self.runtime.pcm_len();
                if post_skip > bounds.soft_cap_samples {
                    let clamp = post_skip - bounds.soft_cap_samples;
                    log::warn!("Audio postprocess: CLAMP after overrun, dropping {} samples", clamp);
                    for _ in 0..clamp {
                        self.runtime.pop_pcm_front();
                    }
                }
            }
            _ => {
                self.stats.catchup_credit = 0.0;
            }
        }

        crossfade_len
    }

    /// Updates buffer state and returns the crossfade length to apply.
    fn update_buffer_state(&mut self, wanted_len: usize, missing: usize) -> usize {
        let now = Instant::now();
        let catchup_elapsed = now.saturating_duration_since(self.stats.catchup_last);
        self.stats.catchup_last = now;
        let policy = self.prepare_buffer_policy(wanted_len, now);
        self.log_refill_bounds(&policy.bounds, policy.next_state, wanted_len, missing);
        let crossfade_len = self.apply_buffer_state(
            policy.next_state,
            &policy.bounds,
            catchup_elapsed,
            policy.one_shot_catchup_active,
        );
        self.buffer_state = policy.next_state;
        crossfade_len
    }

    /// Fills missing samples using a shadow snapshot or last-sample hold.
    fn handle_underrun_fallback(
        &mut self,
        missing: usize,
        chunk: &mut Vec<f32>,
        snapshot: Option<SidLikeSynth>,
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
            if self.buffer_state != next_underrun {
                match next_underrun {
                    BufferState::UnderrunShadow => {
                        self.fallback_snapshot_updates = self.runtime.snapshot_update_count();
                        log::warn!(
                            "Audio postprocess: UNDERRUN! Using shadow synth snapshot (updates={})",
                            self.runtime.snapshot_update_count()
                        );
                    }
                    BufferState::UnderrunHold => {
                        log::warn!(
                            "Audio postprocess: UNDERRUN! No shadow synth snapshot available, holding last sample"
                        );
                    }
                    _ => {}
                }
                self.buffer_state = next_underrun;
            }
            if self.buffer_state == BufferState::UnderrunShadow {
                let current_updates = self.runtime.snapshot_update_count();
                if current_updates != self.fallback_snapshot_updates {
                    self.fallback_snapshot_updates = current_updates;
                    self.fallback_synth_snapshot = self.runtime.snapshot_clone();
                    log::debug!(
                        "Audio postprocess: refreshed shadow snapshot (updates={})",
                        current_updates
                    );
                }
            }
            if let Some(shadow) = self.fallback_synth_snapshot.as_mut() {
                log::trace!("Audio postprocess: rendering {} shadow samples", needed);
                for _ in 0..needed {
                    chunk.push(shadow.render_sample(self.synth_sample_rate));
                }
            } else {
                log::trace!("Audio postprocess: padding {} samples with last-sample hold", needed);
                let hold = chunk.last().copied().unwrap_or(self.last_sample);
                chunk.resize(missing, hold);
            }
        } else if matches!(self.buffer_state, BufferState::UnderrunShadow | BufferState::UnderrunHold) {
            // Real PCM resumed; stop shadow rendering.
            log::debug!("Audio postprocess: PCM buffer recovered, resuming normal output");
            self.fallback_synth_snapshot = None;
            self.buffer_state = BufferState::Normal;
        }
    }

    /// Smooths skip events by crossfading the FIFO tail into the new chunk head.
    fn apply_crossfade_skip(&mut self, crossfade_len: usize, chunk: &mut Vec<f32>) {
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
                    self.runtime.pop_pcm_front();
                }
            }
        } else {
            let fade_from = self.source_fifo.back().copied().unwrap_or(self.last_sample);
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
                    self.runtime.pop_pcm_front();
                }
            }
        }
    }

    /// Applies the pre-resample effect chain in place.
    fn apply_pre_fx_chain(&mut self, chunk: &mut Vec<f32>) {
        if self.pre_fx.is_empty() {
            return;
        }
        for effect in self.pre_fx.iter_mut() {
            effect.prepare(1, self.synth_sample_rate);
            effect.process_in_place(chunk);
        }
    }

    /// Refills the source FIFO with synth PCM or underrun fallback samples.
    fn refill_source_fifo(&mut self, wanted_len: usize) {
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
        let crossfade_len = self.update_buffer_state(wanted_len, missing);

        // Pull PCM from the runtime. If there is not enough, fall back to a snapshot.
        self.runtime.pop_pcm_samples(missing, &mut chunk);
        let snapshot = if chunk.len() < missing && self.fallback_synth_snapshot.is_none() {
            self.runtime.snapshot_clone()
        } else {
            None
        };
        self.handle_underrun_fallback(missing, &mut chunk, snapshot);
        self.apply_crossfade_skip(crossfade_len, &mut chunk);
        self.apply_pre_fx_chain(&mut chunk);

        self.source_fifo.extend(chunk.drain(..));
        self.refill_chunk = chunk;
    }
}
