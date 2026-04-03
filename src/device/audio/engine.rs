//! Cycle-driven audio engine.
//!
//! This module applies CPU-timed register writes to a SID-like synth, renders
//! PCM in sample steps, and resolves MMIO readback requests against historical
//! snapshots.

use crate::device::audio::queue::{LockFreePcmRingBuffer, LockFreeQueue};
use crate::device::audio::AudioConfig;
use crate::device::audio::synth::{AudioRegister, SidLikeSynth};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};


#[derive(Debug, Clone, Copy)]
pub(crate) struct AudioTiming {
    pub cpu_hz: f64,
    pub synth_sample_rate: u32,
    pub target_latency_cycles: f64,
    pub target_latency_samples: usize,
    pub cpu_cycles_per_sample: f64,
}

impl AudioTiming {
    pub fn new(config: AudioConfig) -> Self {
        let synth_sample_rate = config.synth_sample_rate.max(1);
        let cpu_hz = config.cpu_hz.max(1.0);
        let cpu_cycles_per_sample = cpu_hz / synth_sample_rate as f64;
        let target_latency_samples =
            (config.target_latency_cycles / cpu_cycles_per_sample).ceil() as usize;

        Self {
            cpu_hz,
            synth_sample_rate,
            target_latency_cycles: config.target_latency_cycles,
            target_latency_samples,
            cpu_cycles_per_sample,
        }
    }
}

/// A register write scheduled at a CPU cycle.
#[derive(Debug, Clone, Copy)]
pub struct AudioEvent {
    pub cycle: usize,
    pub register: u8,
    pub value: u8,
}

/// Readback state snapshot captured at a rendered cycle.
#[derive(Debug, Clone, Copy)]
pub struct ReadbackPoint {
    pub cycle: usize,
    pub osc3: u8,
    pub env3: u8,
}

/// Lock-free control queues connecting MMIO requests and engine responses.
#[derive(Debug, Clone)]
pub struct AudioControlPlane {
    pub write_events: LockFreeQueue<AudioEvent>,
}

impl AudioControlPlane {
    /// Creates queue set with explicit write and read queue capacities.
    pub fn new(write_queue_capacity: usize) -> Self {
        Self {
            write_events: LockFreeQueue::with_capacity(write_queue_capacity),
        }
    }
}

/// Shared lock-free control queue handle.
pub type SharedAudioControlPlane = Arc<AudioControlPlane>;

/// Shared runtime state exchanged between MMIO, engine, and postprocessor.
#[derive(Debug)]
pub struct AudioDataPlane {
    synth_pcm: LockFreePcmRingBuffer,
    snapshot_synth_state: RwLock<Option<SidLikeSynth>>,
    snapshot_update_count: AtomicU64,
    dropped_stale_events: AtomicU64,
    total_samples_produced: AtomicU64,
    audio_output_enabled: AtomicBool,
    callback_heartbeat_epoch: AtomicU64,
    pcm_soft_cap_samples: AtomicU64,
    target_latency_samples: AtomicU64,
}

impl AudioDataPlane {
    /// Creates shared data-plane storage for stream and snapshot state.
    pub fn new(pcm_capacity: usize) -> Self {
        Self {
            synth_pcm: LockFreePcmRingBuffer::with_capacity(pcm_capacity),
            snapshot_synth_state: RwLock::new(None),
            snapshot_update_count: AtomicU64::new(0),
            dropped_stale_events: AtomicU64::new(0),
            total_samples_produced: AtomicU64::new(0),
            audio_output_enabled: AtomicBool::new(false),
            callback_heartbeat_epoch: AtomicU64::new(0),
            pcm_soft_cap_samples: AtomicU64::new(0),
            target_latency_samples: AtomicU64::new(0),
        }
    }

    /// Appends synthesized PCM samples into the shared stream buffer.
    pub fn push_pcm_samples(&self, samples: &[f32]) {
        self.synth_pcm.push_samples(samples);
        if !samples.is_empty() {
            self.total_samples_produced
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
        }
    }

    /// Pops up to `wanted` PCM samples into `out`.
    pub fn pop_pcm_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        self.synth_pcm.pop_samples(wanted, out);
    }

    /// Removes one front PCM sample if available.
    pub fn pop_pcm_front(&self) {
        self.synth_pcm.pop_front();
    }

    /// Removes all queued PCM samples.
    pub fn clear_pcm_buffer(&self) {
        while self.synth_pcm.len() > 0 {
            self.synth_pcm.pop_front();
        }
    }

    /// Records that the host output callback ran.
    pub fn note_callback_activity(&self) {
        let _ = self.callback_heartbeat_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the last observed host callback heartbeat.
    pub fn callback_heartbeat_epoch(&self) -> u64 {
        self.callback_heartbeat_epoch.load(Ordering::Relaxed)
    }

    /// Returns buffered PCM sample count.
    pub fn pcm_len(&self) -> usize {
        self.synth_pcm.len()
    }

    /// Returns PCM staging buffer capacity in samples.
    pub fn pcm_capacity_samples(&self) -> usize {
        self.synth_pcm.capacity()
    }

    /// Returns a cloned synth snapshot for underrun fallback if available.
    pub fn snapshot_clone(&self) -> Option<SidLikeSynth> {
        self.snapshot_synth_state.read().unwrap().clone()
    }

    /// Stores a fresh synth snapshot and increments update counter.
    pub fn store_snapshot(&self, synth: SidLikeSynth) {
        *self.snapshot_synth_state.write().unwrap() = Some(synth);
        self.snapshot_update_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns snapshot update counter.
    pub fn snapshot_update_count(&self) -> u64 {
        self.snapshot_update_count.load(Ordering::Relaxed)
    }

    /// Returns number of stale control events dropped by the engine.
    pub fn dropped_stale_events(&self) -> u64 {
        self.dropped_stale_events.load(Ordering::Relaxed)
    }

    /// Returns total number of PCM samples produced since startup.
    pub fn total_samples_produced(&self) -> u64 {
        self.total_samples_produced.load(Ordering::Relaxed)
    }

    /// Returns total failed reads due to underrun.
    pub fn underrun_samples(&self) -> u64 {
        self.synth_pcm.underrun_samples()
    }

    /// Enables or disables PCM rendering for the current audio session.
    pub fn set_audio_output_enabled(&self, enabled: bool) {
        self.audio_output_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Returns whether the audio session is currently enabled.
    pub fn audio_output_enabled(&self) -> bool {
        self.audio_output_enabled.load(Ordering::Relaxed)
    }

    /// Sets a soft cap for PCM buffering; 0 disables the cap hint.
    pub fn set_pcm_soft_cap_samples(&self, samples: usize) {
        self.pcm_soft_cap_samples
            .store(samples as u64, Ordering::Relaxed);
    }

    /// Returns the configured soft cap for PCM buffering, or 0 if unset.
    pub fn pcm_soft_cap_samples(&self) -> usize {
        self.pcm_soft_cap_samples.load(Ordering::Relaxed) as usize
    }

    /// Prefills the PCM buffer with silence samples.
    pub fn prefill_pcm_silence(&self, samples: usize) {
        if samples == 0 {
            return;
        }
        let zeros = vec![0.0; samples];
        self.push_pcm_samples(&zeros);
    }

    /// Stores the configured target latency in synth samples.
    pub fn set_target_latency_samples(&self, samples: usize) {
        self.target_latency_samples
            .store(samples as u64, Ordering::Relaxed);
    }

    /// Returns the target latency in synth samples, or 0 if unset.
    pub fn target_latency_samples(&self) -> usize {
        self.target_latency_samples.load(Ordering::Relaxed) as usize
    }

    /// Adds dropped stale event count.
    pub fn add_dropped_stale_events(&self, count: u64) {
        self.dropped_stale_events.fetch_add(count, Ordering::Relaxed);
    }
}

/// Shared data-plane handle used across audio components.
pub type SharedAudioDataPlane = Arc<AudioDataPlane>;

/// Synth render engine that advances according to CPU cycle progression.
#[derive(Debug, Clone)]
pub struct AudioEngine {
    runtime: SharedAudioDataPlane,
    control_queues: SharedAudioControlPlane,
    synth: SidLikeSynth,
    pending_writes: VecDeque<AudioEvent>,
    scratch_events: Vec<AudioEvent>,
    readback_history: VecDeque<ReadbackPoint>,
    drained_write_events: Vec<AudioEvent>,
    produced_samples: Vec<f32>,
    produced_readbacks: Vec<ReadbackPoint>,
    synth_sample_rate: u32,
    cpu_cycles_per_sample: f64,
    rendered_cpu_cycle: f64,
    target_latency_cycles: f64,
    history_horizon_cycles: usize,
    max_pending_writes: usize,
    snapshot_dirty_since_capture: bool,
    stats: PcmStats,
    last_cpu_cycle: usize,
    seen_audio_output_enabled: bool,
}

#[derive(Debug, Clone)]
struct PcmStats {
    stats_last: Instant,
    stats_samples: u64,
    debug_log_last: Instant,
}

impl AudioEngine {
    /// Creates a new engine with timing parameters for CPU and synth domains.
    pub fn new(
        runtime: SharedAudioDataPlane,
        control_queues: SharedAudioControlPlane,
        config: AudioConfig,
    ) -> Self {
        let timing = AudioTiming::new(config);
        log::info!(
            "Audio engine timing: cpu_hz={:.2}, synth_hz={}, cycles_per_sample={:.3}, target_latency_cycles={:.1} (~{} samples)",
            timing.cpu_hz,
            timing.synth_sample_rate,
            timing.cpu_cycles_per_sample,
            timing.target_latency_cycles,
            timing.target_latency_samples
        );

        runtime.set_target_latency_samples(timing.target_latency_samples);

        let engine = Self {
            runtime: runtime.clone(),
            control_queues,
            synth: SidLikeSynth::new(),
            pending_writes: VecDeque::new(),
            scratch_events: Vec::new(),
            readback_history: VecDeque::new(),
            drained_write_events: Vec::new(),
            produced_samples: Vec::new(),
            produced_readbacks: Vec::new(),
            synth_sample_rate: timing.synth_sample_rate,
            cpu_cycles_per_sample: timing.cpu_cycles_per_sample,
            rendered_cpu_cycle: 0.0,
            target_latency_cycles: timing.target_latency_cycles.max(0.0),
            history_horizon_cycles: (timing.cpu_hz as usize).max(8192),
            max_pending_writes: 8192,
            snapshot_dirty_since_capture: false,
            stats: PcmStats {
                stats_last: Instant::now(),
                stats_samples: 0,
                debug_log_last: Instant::now(),
            },
            last_cpu_cycle: 0,
            seen_audio_output_enabled: runtime.audio_output_enabled(),
        };

        // Seed a baseline snapshot so underrun fallback has a synth state immediately.
        engine.runtime.store_snapshot(engine.synth.clone());
        engine
    }

    /// Returns the synth sample rate produced by this engine.
    pub const fn synth_sample_rate(&self) -> u32 {
        self.synth_sample_rate
    }

    /// Advances synthesis toward `cpu_cycle` while maintaining target latency.
    pub fn advance_to_cpu_cycle(&mut self, cpu_cycle: usize) {
        self.last_cpu_cycle = cpu_cycle;
        let target_cycle = (cpu_cycle as f64 - self.target_latency_cycles).max(0.0);
        self.advance_to_target_cycle(target_cycle);
    }

    /// Returns the latest readback snapshot at or before `cycle`.
    fn readback_for_cycle(&self, cycle: usize, register: u8) -> Option<u8> {
        for point in self.readback_history.iter().rev() {
            if point.cycle <= cycle {
                return Some(match register {
                    r if r == AudioRegister::Osc3Read.as_u8() => point.osc3,
                    r if r == AudioRegister::Env3Read.as_u8() => point.env3,
                    _ => 0,
                });
            }
        }
        None
    }

    /// Resolves a readback without advancing the main audio timeline.
    pub fn resolve_readback_value(&mut self, cycle: usize, register: u8, max_cycle: usize) -> u8 {
        let target_cycle = cycle.min(max_cycle);
        self.drain_write_events();

        let last_sample_cycle = self.last_sample_cycle();
        if self.rendered_cpu_cycle == 0.0 && !self.pending_writes.is_empty() {
            return self.readback_from_scratch(target_cycle, register);
        }
        if target_cycle <= last_sample_cycle {
            return self
                .readback_for_cycle(target_cycle, register)
                .unwrap_or_else(|| Self::readback_from_synth(&self.synth, register));
        }

        self.readback_from_scratch(target_cycle, register)
    }

    // Replays writes and renders synth state forward *on a scrap copy* from the last sample to resolve readback.
    // Does not modify main synth state or timeline, allowing readback resolution without affecting output timing.
    fn readback_from_scratch(&mut self, target_cycle: usize, register: u8) -> u8 {
        let last_sample_cycle = self.last_sample_cycle();
        let mut scratch = self.synth.clone();
        let mut scratch_cycle = self.rendered_cpu_cycle;

        let min_cycle = if self.rendered_cpu_cycle == 0.0 {
            0
        } else {
            last_sample_cycle.saturating_add(1)
        };

        self.scratch_events.clear();
        self.scratch_events.extend(
            self.pending_writes
                .iter()
                .copied()
                .filter(|event| event.cycle >= min_cycle && event.cycle <= target_cycle),
        );
        self.scratch_events.sort_by_key(|event| event.cycle);
        let events = &self.scratch_events;

        let mut idx = 0usize;
        let target_cycle_f = target_cycle as f64;
        while scratch_cycle + self.cpu_cycles_per_sample <= target_cycle_f {
            let sample_cycle = scratch_cycle as usize;
            while idx < events.len() && events[idx].cycle <= sample_cycle {
                let event = events[idx];
                scratch.write_register(event.register, event.value);
                idx += 1;
            }
            let _ = scratch.render_sample(self.synth_sample_rate);
            scratch_cycle += self.cpu_cycles_per_sample;
        }

        Self::readback_from_synth(&scratch, register)
    }

    fn readback_from_synth(synth: &SidLikeSynth, register: u8) -> u8 {
        match register {
            r if r == AudioRegister::Osc3Read.as_u8() => synth.osc3_readback(),
            r if r == AudioRegister::Env3Read.as_u8() => synth.env3_readback(),
            _ => synth.read_register(register),
        }
    }

    fn last_sample_cycle(&self) -> usize {
        if self.rendered_cpu_cycle <= 0.0 {
            return 0;
        }
        let last_cycle = (self.rendered_cpu_cycle - self.cpu_cycles_per_sample).max(0.0);
        last_cycle as usize
    }

    fn drain_write_events(&mut self) {
        self.control_queues
            .write_events
            .drain_into(&mut self.drained_write_events);
        let write_count = self.drained_write_events.len();
        let now = Instant::now();
        if write_count > 0 && self.should_log_debug(now) {
            log::debug!("Audio engine: {} write events drained", write_count);
        }
        self.pending_writes.reserve(write_count);
        for event in self.drained_write_events.drain(..) {
            self.pending_writes.push_back(event);
        }
    }

    fn should_log_debug(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.stats.debug_log_last) >= Duration::from_secs(1) {
            self.stats.debug_log_last = now;
            true
        } else {
            false
        }
    }

    fn compute_target_cycle(&mut self, desired_target_cycle: f64) -> (f64, bool) {
        // Apply backpressure based on the soft cap so we don't overfill the shared PCM ring.
        let target_latency_samples = (self.target_latency_cycles / self.cpu_cycles_per_sample)
            .ceil() as usize;
        let mut max_buffered = self.runtime.pcm_soft_cap_samples();
        if max_buffered == 0 {
            max_buffered = target_latency_samples.max(1).saturating_mul(2);
        }
        max_buffered = max_buffered.max(target_latency_samples.max(1));
        let buffered = self.runtime.pcm_len();
        let allowed_samples = max_buffered.saturating_sub(buffered);
        let now = Instant::now();
        let log_ready = self.should_log_debug(now);

        if allowed_samples == 0 {
            if log_ready {
                log::debug!(
                    "Audio engine backpressure: buffered={} cap={} -> skipping render",
                    buffered,
                    max_buffered
                );
            }
            return (self.rendered_cpu_cycle, log_ready);
        }

        let max_target_cycle = self.rendered_cpu_cycle
            + allowed_samples as f64 * self.cpu_cycles_per_sample;
        let clamped = desired_target_cycle.min(max_target_cycle);
        if clamped < desired_target_cycle && log_ready {
            log::debug!(
                "Audio engine clamp: buffered={} cap={} allowed={} target_cycle={:.2}->{:.2}",
                buffered,
                max_buffered,
                allowed_samples,
                desired_target_cycle,
                clamped
            );
        }

        (clamped, log_ready)
    }

    fn apply_pending_writes(&mut self, sample_cycle: usize) {
        while let Some(event) = self.pending_writes.front().copied() {
            if event.cycle > sample_cycle {
                break;
            }
            let _ = self.pending_writes.pop_front();
            self.synth.write_register(event.register, event.value);
            self.snapshot_dirty_since_capture = true;
        }
    }

    fn record_readbacks(&mut self) {
        let rendered_cycle = self.rendered_cpu_cycle as usize;
        let history_min_cycle = rendered_cycle.saturating_sub(self.history_horizon_cycles);

        for point in self.produced_readbacks.drain(..) {
            self.readback_history.push_back(point);
        }

        while self
            .readback_history
            .front()
            .is_some_and(|p| p.cycle < history_min_cycle)
        {
            let _ = self.readback_history.pop_front();
        }
    }

    fn drop_stale_writes(&mut self) -> u64 {
        let rendered_cycle = self.rendered_cpu_cycle as usize;
        let history_min_cycle = rendered_cycle.saturating_sub(self.history_horizon_cycles);
        let mut dropped_stale = 0u64;

        while self.pending_writes.len() > self.max_pending_writes {
            let Some(front) = self.pending_writes.front().copied() else {
                break;
            };
            if front.cycle > history_min_cycle {
                break;
            }
            let _ = self.pending_writes.pop_front();
            dropped_stale = dropped_stale.saturating_add(1);
        }

        dropped_stale
    }

    fn flush_produced_samples(&mut self) -> usize {
        let sample_count = self.produced_samples.len();
        if sample_count == 0 {
            return 0;
        }

        self.runtime.push_pcm_samples(&self.produced_samples);

        // Capture a fallback synth snapshot after the first render or when state changes.
        if self.snapshot_dirty_since_capture || self.runtime.snapshot_update_count() == 0 {
            self.runtime.store_snapshot(self.synth.clone());
            self.snapshot_dirty_since_capture = false;
        }

        sample_count
    }

    fn apply_pending_writes_until(&mut self, cycle: usize) {
        while let Some(event) = self.pending_writes.front().copied() {
            if event.cycle > cycle {
                break;
            }
            let _ = self.pending_writes.pop_front();
            self.synth.write_register(event.register, event.value);
            self.snapshot_dirty_since_capture = true;
        }
    }

    /// Internal render loop that applies due writes and emits samples/readbacks.
    fn advance_to_target_cycle(&mut self, target_cycle: f64) {
        self.drain_write_events();
        let output_enabled = self.runtime.audio_output_enabled();
        if output_enabled != self.seen_audio_output_enabled {
            self.seen_audio_output_enabled = output_enabled;
            if output_enabled {
                let target_cycle_usize = target_cycle.max(0.0) as usize;
                self.apply_pending_writes_until(target_cycle_usize);
                self.rendered_cpu_cycle = target_cycle.max(self.rendered_cpu_cycle);
                self.readback_history.clear();
                self.runtime.store_snapshot(self.synth.clone());
                self.snapshot_dirty_since_capture = false;
                log::info!(
                    "Audio engine session resumed at cycle {:.1}",
                    self.rendered_cpu_cycle
                );
            }
        }

        if !output_enabled {
            self.apply_pending_writes_until(target_cycle.max(0.0) as usize);
            self.last_cpu_cycle = target_cycle.max(0.0) as usize;
            return;
        }

        let (target_cycle, log_ready) = self.compute_target_cycle(target_cycle);

        // Batch produced samples/readbacks to amortize queue operations.
        self.produced_samples.clear();
        self.produced_readbacks.clear();

        let remaining_cycles = (target_cycle - self.rendered_cpu_cycle).max(0.0);
        let estimated_steps = (remaining_cycles / self.cpu_cycles_per_sample).ceil() as usize;
        if estimated_steps > 0 && log_ready {
            log::trace!(
                "Audio engine: advancing {} samples (remaining_cycles={:.1})",
                estimated_steps,
                remaining_cycles
            );
        }
        self.produced_samples.reserve(estimated_steps);
        self.produced_readbacks.reserve(estimated_steps);

        while self.rendered_cpu_cycle + self.cpu_cycles_per_sample <= target_cycle {
            let sample_cycle = self.rendered_cpu_cycle as usize;
            self.apply_pending_writes(sample_cycle);

            let sample = self.synth.render_sample(self.synth_sample_rate);
            self.produced_samples.push(sample);

            self.produced_readbacks.push(ReadbackPoint {
                cycle: sample_cycle,
                osc3: self.synth.osc3_readback(),
                env3: self.synth.env3_readback(),
            });

            self.rendered_cpu_cycle += self.cpu_cycles_per_sample;
        }

        if !self.produced_samples.is_empty() {
            self.stats.stats_samples = self
                .stats.stats_samples
                .saturating_add(self.produced_samples.len() as u64);
        }
        self.record_readbacks();
        let dropped_stale = self.drop_stale_writes();

        if self.produced_samples.is_empty() && dropped_stale == 0 {
            return;
        }

        let sample_count = self.flush_produced_samples();
        if sample_count > 0 {
            log::trace!(
                "Audio engine step: {} samples produced, total queued PCM: {}",
                sample_count,
                self.runtime.pcm_len()
            );
        }

        let stats_elapsed = self.stats.stats_last.elapsed();
        if stats_elapsed >= Duration::from_secs(1) {
            let secs = stats_elapsed.as_secs_f64().max(0.001);
            let samples_per_sec = self.stats.stats_samples as f64 / secs;
            log::info!(
                "Audio engine stats: cpu_cycle={}, rendered_cycle={:.1}, target_cycle={:.1}, buffered={}, samples/s={:.1}, pending_writes={}, dropped_stale={}",
                self.last_cpu_cycle,
                self.rendered_cpu_cycle,
                target_cycle,
                self.runtime.pcm_len(),
                samples_per_sec,
                self.pending_writes.len(),
                self.runtime.dropped_stale_events()
            );
            self.stats.stats_last = Instant::now();
            self.stats.stats_samples = 0;
        }

        if dropped_stale > 0 {
            log::warn!(
                "Audio engine: dropped {} stale write events (pending_writes overflow)",
                dropped_stale
            );
            self.runtime.add_dropped_stale_events(dropped_stale);
        }
    }
}
