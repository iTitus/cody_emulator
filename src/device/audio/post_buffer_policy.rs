//! Buffer management policy for the audio postprocessor.

use crate::device::audio::source::PcmSource;
use std::sync::Once;
use std::time::{Duration, Instant};
use super::CONTACT;

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
    configured_latency_samples: usize,
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
struct BufferPolicyStats {
    catchup_credit: f64,
    catchup_last: Instant,
    start_instant: Instant,
    initial_gate_open: bool,
    one_shot_catchup_done: bool,
    one_shot_catchup_forced_off: bool,
}

/// Buffer management helper used by the postprocessor.
#[derive(Debug, Clone)]
pub struct BufferPolicyManager {
    buffer_state: BufferState,
    stats: BufferPolicyStats,
    initial_catchup_enabled: bool,
}

impl BufferPolicyManager {
    /// Creates a buffer policy manager with startup catchup tracking.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            buffer_state: BufferState::Normal,
            stats: BufferPolicyStats {
                catchup_credit: 0.0,
                catchup_last: now,
                start_instant: now,
                initial_gate_open: false,
                one_shot_catchup_done: false,
                one_shot_catchup_forced_off: false,
            },
            initial_catchup_enabled: true,
        }
    }

    /// Enables or disables the initial catch-up sensitivity heuristic.
    pub fn set_initial_catchup_enabled(&mut self, enabled: bool) {
        if self.initial_catchup_enabled == enabled {
            return;
        }

        self.initial_catchup_enabled = enabled;
        if !enabled {
            self.stats.one_shot_catchup_forced_off = !self.stats.one_shot_catchup_done;
            self.stats.one_shot_catchup_done = true;
        } else if self.stats.one_shot_catchup_forced_off {
            self.stats.one_shot_catchup_done = false;
            self.stats.one_shot_catchup_forced_off = false;
        }
    }

    /// Returns whether initial catch-up sensitivity is enabled.
    pub fn initial_catchup_enabled(&self) -> bool {
        self.initial_catchup_enabled
    }

    /// Returns the current buffer management state.
    pub fn buffer_state(&self) -> BufferState {
        self.buffer_state
    }

    /// Overrides the current buffer state (used for underrun fallback transitions).
    pub fn set_buffer_state(&mut self, state: BufferState) {
        self.buffer_state = state;
    }

    /// Predicts the next buffer state without consuming any PCM.
    pub fn preview_buffer_state(
        &mut self,
        runtime: &dyn PcmSource,
        synth_sample_rate: u32,
        wanted_len: usize,
    ) -> BufferState {
        let now = Instant::now();
        let policy = self.prepare_buffer_policy(runtime, synth_sample_rate, wanted_len, now);
        policy.next_state
    }

    /// Applies buffer policy for a refill and returns a crossfade length.
    pub fn update_buffer_state(
        &mut self,
        runtime: &dyn PcmSource,
        synth_sample_rate: u32,
        wanted_len: usize,
        missing: usize,
    ) -> usize {
        let now = Instant::now();
        let catchup_elapsed = now.saturating_duration_since(self.stats.catchup_last);
        self.stats.catchup_last = now;
        let policy = self.prepare_buffer_policy(runtime, synth_sample_rate, wanted_len, now);
        self.log_refill_bounds(&policy.bounds, policy.next_state, wanted_len, missing);
        let crossfade_len = self.apply_buffer_state(
            runtime,
            policy.next_state,
            &policy.bounds,
            catchup_elapsed,
            policy.one_shot_catchup_active,
        );
        self.buffer_state = policy.next_state;
        crossfade_len
    }

    /// Computes latency targets and thresholds derived from runtime state.
    fn compute_buffer_bounds(
        &self,
        runtime: &dyn PcmSource,
        synth_sample_rate: u32,
        wanted_len: usize,
    ) -> BufferBounds {
        let configured_latency_samples = runtime.target_latency_samples();
        let mut soft_cap_samples = runtime.pcm_soft_cap_samples();
        if soft_cap_samples == 0 {
            soft_cap_samples = wanted_len.saturating_mul(4);
        } else {
            log::trace!(
                "Audio postprocess: using soft cap {} samples",
                soft_cap_samples
            );
        }
        let buffered_samples = runtime.pcm_len();
        let frame_samples = synth_sample_rate.saturating_div(60).max(1) as usize;
        let drift_guard_samples = if configured_latency_samples > 0 {
            let guard = configured_latency_samples + frame_samples + frame_samples / 4;
            guard.max(wanted_len)
        } else {
            wanted_len
        };
        let over_soft_cap = buffered_samples > soft_cap_samples;
        let target_buffer_samples = if configured_latency_samples > 0 {
            configured_latency_samples.max(wanted_len)
        } else {
            wanted_len
        };
        let max_crossfade_samples = frame_samples.saturating_div(2).max(64);

        BufferBounds {
            configured_latency_samples,
            soft_cap_samples,
            buffered_samples,
            frame_samples,
            drift_guard_samples,
            over_soft_cap,
            target_buffer_samples,
            max_crossfade_samples,
        }
    }

    /// Logs state transitions once per distinct change.
    fn log_state_change(&self, next_state: BufferState, bounds: &BufferBounds) {
        if next_state != self.buffer_state {
            static BUFFER_STATE_NOTICE: Once = Once::new();
            BUFFER_STATE_NOTICE.call_once(|| {
                log::warn!(r#"Audio:
                Notice for the interested reader about audio:
                * BUFFER STATE changes indicate buffer management actions:
                    - Normal: Buffer is healthy; normal PCM output, no special handling.
                    - Catchup: Buffer is above target; gradually skip old samples to catch up, with crossade.
                    - Overrun, UnderrunShadow, UnderrunHold: Buffer is far above soft cap or underrun; aggressive actions taken to avoid latency or generate fallback audio.
                    If you see frequent or persistent Catchup/Overrun states, that may indicate performance issues or misconfigured latency targets.
                * Occasional BUFFER STATE transitions are fine. Seeing one early during emulation is normal. The audio engine tries to reduce latency early that way.
                * If you find some weird audio behaviour, contact the author of the emulator or, even better, of the audio subsystem ({CONTACT}).
                * To disable audio entirely, pass --audio-off. For more options, see --help."#);
            });
            log::warn!(
                "Audio postprocess: BUFFER STATE CHANGE {:?} -> {:?} (buffered={}, configured_latency={}, target_buffer={}, drift_guard={}, soft_cap={})",
                self.buffer_state,
                next_state,
                bounds.buffered_samples,
                bounds.configured_latency_samples,
                bounds.target_buffer_samples,
                bounds.drift_guard_samples,
                bounds.soft_cap_samples
            );
        }
    }

    /// Chooses the next state and whether one-shot catchup is still active.
    fn select_buffer_state(&mut self, bounds: &BufferBounds, now: Instant) -> (BufferState, bool) {
        if !self.stats.initial_gate_open
            && now.saturating_duration_since(self.stats.start_instant) >= Duration::from_secs(1)
        {
            self.stats.initial_gate_open = true;
        }

        if self.stats.initial_gate_open
            && !self.stats.one_shot_catchup_done
            && bounds.buffered_samples <= bounds.target_buffer_samples
        {
            self.stats.one_shot_catchup_done = true;
            self.stats.one_shot_catchup_forced_off = false;
        }

        let one_shot_catchup_active = self.initial_catchup_enabled
            && self.stats.initial_gate_open
            && !self.stats.one_shot_catchup_done;

        let drift_guard = bounds.drift_guard_samples.max(bounds.target_buffer_samples);
        let one_shot_threshold = bounds.target_buffer_samples
            + drift_guard.saturating_sub(bounds.target_buffer_samples) / 2;

        let catchup_requested = self.stats.initial_gate_open
            && !bounds.over_soft_cap
            && if self.buffer_state == BufferState::Catchup {
                bounds.buffered_samples > bounds.target_buffer_samples
            } else if one_shot_catchup_active {
                bounds.buffered_samples > one_shot_threshold
            } else {
                bounds.buffered_samples > drift_guard
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

    /// Packages bounds and next state for the current refill decision.
    fn prepare_buffer_policy(
        &mut self,
        runtime: &dyn PcmSource,
        synth_sample_rate: u32,
        wanted_len: usize,
        now: Instant,
    ) -> BufferPolicy {
        let bounds = self.compute_buffer_bounds(runtime, synth_sample_rate, wanted_len);
        let (next_state, one_shot_catchup_active) = self.select_buffer_state(&bounds, now);
        BufferPolicy {
            bounds,
            next_state,
            one_shot_catchup_active,
        }
    }

    /// Logs refill thresholds and the selected state for tracing.
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
            bounds.configured_latency_samples,
            bounds.target_buffer_samples,
            bounds.drift_guard_samples,
            bounds.soft_cap_samples,
            self.buffer_state,
            next_state,
            wanted_len,
            missing
        );
    }

    /// Applies the buffer policy and returns the requested crossfade length.
    fn apply_buffer_state(
        &mut self,
        runtime: &dyn PcmSource,
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
                let catchup_secs = 0.5_f64;
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
                        runtime.pop_pcm_front();
                    }
                    self.stats.catchup_credit -= to_skip as f64;
                    crossfade_len = crossfade_len.max(planned_fade);
                    let post_skip = runtime.pcm_len();
                    if one_shot_catchup_active && post_skip <= bounds.target_buffer_samples {
                        self.stats.one_shot_catchup_done = true;
                        self.stats.one_shot_catchup_forced_off = false;
                    }
                }
                let post_skip = runtime.pcm_len();
                if post_skip > bounds.soft_cap_samples {
                    let clamp = post_skip - bounds.soft_cap_samples;
                    log::warn!(
                        "Audio postprocess: CLAMP after catchup, dropping {} samples",
                        clamp
                    );
                    for _ in 0..clamp {
                        runtime.pop_pcm_front();
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
                        runtime.pop_pcm_front();
                    }
                }
                let post_skip = runtime.pcm_len();
                if post_skip > bounds.soft_cap_samples {
                    let clamp = post_skip - bounds.soft_cap_samples;
                    log::warn!(
                        "Audio postprocess: CLAMP after overrun, dropping {} samples",
                        clamp
                    );
                    for _ in 0..clamp {
                        runtime.pop_pcm_front();
                    }
                }
            }
            _ => {
                self.stats.catchup_credit = 0.0;
            }
        }

        crossfade_len
    }
}
