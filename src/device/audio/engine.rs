//! Cycle-driven audio engine.
//!
//! This module applies CPU-timed register writes to a SID-like synth, renders
//! PCM in sample steps, and resolves MMIO readback requests against historical
//! snapshots.

use crate::device::audio::queue::{EventQueueHandle, PcmBufferHandle, new_event_queue, new_pcm_buffer};
use crate::device::audio::AudioConfig;
use crate::device::audio::compute_soft_cap_samples;
use crate::device::audio::registers::AudioRegister;
use crate::device::audio::synth::SidLikeSynth;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};


#[derive(Debug, Clone, Copy)]
pub struct AudioTiming {
    pub cpu_hz: f64,
    pub synth_sample_rate: u32,
    pub target_latency_cycles: f64,
    pub target_latency_samples: usize,
    pub cpu_cycles_per_sample: f64,
}

impl AudioTiming {
    /// Derives timing ratios and latency targets from the runtime config.
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

/// Readback inputs borrowed from the current engine state.
struct ReadbackContext<'a> {
    synth: &'a SidLikeSynth,
    pending_writes: &'a VecDeque<AudioEvent>,
    rendered_cpu_cycle: f64,
    cpu_cycles_per_sample: f64,
    synth_sample_rate: u32,
}

impl<'a> ReadbackContext<'a> {
    /// Returns the CPU cycle corresponding to the last rendered sample.
    fn last_sample_cycle(&self) -> usize {
        if self.rendered_cpu_cycle <= 0.0 {
            return 0;
        }
        let last_cycle = (self.rendered_cpu_cycle - self.cpu_cycles_per_sample).max(0.0);
        last_cycle as usize
    }
}

/// Readback history and scratch replay state used by the engine.
#[derive(Debug, Clone)]
struct ReadbackResolver {
    history: VecDeque<ReadbackPoint>,
    produced: Vec<ReadbackPoint>,
    scratch_events: Vec<AudioEvent>,
    history_horizon_cycles: usize,
}

impl ReadbackResolver {
    /// Creates a readback resolver with a history horizon in CPU cycles.
    fn new(history_horizon_cycles: usize) -> Self {
        Self {
            history: VecDeque::new(),
            produced: Vec::new(),
            scratch_events: Vec::new(),
            history_horizon_cycles,
        }
    }

    /// Updates the maximum history age in CPU cycles.
    fn set_history_horizon(&mut self, history_horizon_cycles: usize) {
        self.history_horizon_cycles = history_horizon_cycles;
    }

    /// Clears any pending readbacks from the current render batch.
    fn clear_batch(&mut self) {
        self.produced.clear();
    }

    /// Reserves space for the next batch of readback points.
    fn reserve_batch(&mut self, count: usize) {
        self.produced.reserve(count);
    }

    /// Captures a readback point from the synth at a given CPU cycle.
    fn push_readback(&mut self, cycle: usize, synth: &SidLikeSynth) {
        self.produced.push(ReadbackPoint {
            cycle,
            osc3: synth.osc3_readback(),
            env3: synth.env3_readback(),
        });
    }

    /// Flushes the current batch into history and prunes old entries.
    fn flush(&mut self, rendered_cpu_cycle: f64) {
        let rendered_cycle = rendered_cpu_cycle as usize;
        let history_min_cycle = rendered_cycle.saturating_sub(self.history_horizon_cycles);

        for point in self.produced.drain(..) {
            self.history.push_back(point);
        }

        while self
            .history
            .front()
            .is_some_and(|p| p.cycle < history_min_cycle)
        {
            let _ = self.history.pop_front();
        }
    }

    /// Resolves readbacks using history or a scratch replay of writes.
    fn resolve_readback_value(
        &mut self,
        cycle: usize,
        register: u8,
        max_cycle: usize,
        ctx: ReadbackContext<'_>,
    ) -> u8 {
        let target_cycle = cycle.min(max_cycle);
        let last_sample_cycle = ctx.last_sample_cycle();
        if ctx.rendered_cpu_cycle == 0.0 && !ctx.pending_writes.is_empty() {
            return self.readback_from_scratch(target_cycle, register, ctx);
        }
        if target_cycle <= last_sample_cycle {
            return self
                .readback_for_cycle(target_cycle, register)
                .unwrap_or_else(|| Self::readback_from_synth(ctx.synth, register));
        }

        self.readback_from_scratch(target_cycle, register, ctx)
    }

    /// Returns the last known readback at or before the given cycle.
    fn readback_for_cycle(&self, cycle: usize, register: u8) -> Option<u8> {
        for point in self.history.iter().rev() {
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

    /// Replays writes on a scratch synth to resolve readbacks without output.
    fn readback_from_scratch(
        &mut self,
        target_cycle: usize,
        register: u8,
        ctx: ReadbackContext<'_>,
    ) -> u8 {
        let last_sample_cycle = ctx.last_sample_cycle();
        let mut scratch = ctx.synth.clone();
        let mut scratch_cycle = ctx.rendered_cpu_cycle;

        let min_cycle = if ctx.rendered_cpu_cycle == 0.0 {
            0
        } else {
            last_sample_cycle.saturating_add(1)
        };

        self.scratch_events.clear();
        self.scratch_events.extend(
            ctx.pending_writes
                .iter()
                .copied()
                .filter(|event| event.cycle >= min_cycle && event.cycle <= target_cycle),
        );
        self.scratch_events.sort_by_key(|event| event.cycle);
        let events = &self.scratch_events;

        let mut idx = 0usize;
        let target_cycle_f = target_cycle as f64;
        while scratch_cycle + ctx.cpu_cycles_per_sample <= target_cycle_f {
            let sample_cycle = scratch_cycle as usize;
            while idx < events.len() && events[idx].cycle <= sample_cycle {
                let event = events[idx];
                scratch.write_register(event.register, event.value);
                idx += 1;
            }
            let _ = scratch.render_sample(ctx.synth_sample_rate);
            scratch_cycle += ctx.cpu_cycles_per_sample;
        }

        Self::readback_from_synth(&scratch, register)
    }

    /// Reads osc/env readbacks or raw register values from a synth.
    fn readback_from_synth(synth: &SidLikeSynth, register: u8) -> u8 {
        match register {
            r if r == AudioRegister::Osc3Read.as_u8() => synth.osc3_readback(),
            r if r == AudioRegister::Env3Read.as_u8() => synth.env3_readback(),
            _ => synth.read_register(register),
        }
    }
}

/// Lock-free control queues connecting MMIO requests and engine responses.
#[derive(Clone)]
pub struct AudioControlPlane {
    pub write_events: EventQueueHandle<AudioEvent>,
}

impl fmt::Debug for AudioControlPlane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioControlPlane")
            .field("write_events_len", &self.write_events.len())
            .field("write_events_capacity", &self.write_events.capacity())
            .field("write_events_overruns", &self.write_events.overrun_count())
            .finish()
    }
}

impl AudioControlPlane {
    /// Creates queue set with explicit write and read queue capacities.
    pub fn new(write_queue_capacity: usize) -> Self {
        Self {
            write_events: new_event_queue(write_queue_capacity),
        }
    }
}

/// Shared lock-free control queue handle.
pub type SharedAudioControlPlane = Arc<AudioControlPlane>;

/// Shared runtime state exchanged between MMIO, engine, and postprocessor.
pub struct AudioDataPlane {
    pcm: PcmBufferHandle,
    snapshot_synth_state: RwLock<Option<SidLikeSynth>>,
    snapshot_update_count: AtomicU64,
    dropped_stale_events: AtomicU64,
    total_samples_produced: AtomicU64,
    audio_output_enabled: AtomicBool,
    callback_heartbeat_epoch: AtomicU64,
    pcm_soft_cap_samples: AtomicU64,
    target_latency_samples: AtomicU64,
    stats: Mutex<AudioStats>,
}

impl fmt::Debug for AudioDataPlane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioDataPlane")
            .field("pcm_len", &self.pcm.len())
            .field("pcm_capacity", &self.pcm.capacity())
            .field("snapshot_update_count", &self.snapshot_update_count())
            .field("dropped_stale_events", &self.dropped_stale_events())
            .field("total_samples_produced", &self.total_samples_produced())
            .field("pcm_soft_cap_samples", &self.pcm_soft_cap_samples())
            .field("target_latency_samples", &self.target_latency_samples())
            .finish()
    }
}

#[derive(Debug, Clone)]
struct AudioStats {
    last_engine_stats: Instant,
    last_engine_debug: Instant,
    last_postprocess: Instant,
}

impl AudioDataPlane {
    /// Creates shared data-plane storage for stream and snapshot state.
    pub fn new(pcm_capacity: usize) -> Self {
        let now = Instant::now();
        Self {
            pcm: new_pcm_buffer(pcm_capacity),
            snapshot_synth_state: RwLock::new(None),
            snapshot_update_count: AtomicU64::new(0),
            dropped_stale_events: AtomicU64::new(0),
            total_samples_produced: AtomicU64::new(0),
            audio_output_enabled: AtomicBool::new(false),
            callback_heartbeat_epoch: AtomicU64::new(0),
            pcm_soft_cap_samples: AtomicU64::new(0),
            target_latency_samples: AtomicU64::new(0),
            stats: Mutex::new(AudioStats {
                last_engine_stats: now,
                last_engine_debug: now,
                last_postprocess: now,
            }),
        }
    }

    /// Appends synthesized PCM samples into the shared stream buffer.
    pub fn push_pcm_samples(&self, samples: &[f32]) {
        self.pcm.push_samples(samples);
        if !samples.is_empty() {
            self.total_samples_produced
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
        }
    }

    /// Pops up to `wanted` PCM samples into `out`.
    pub fn pop_pcm_samples(&self, wanted: usize, out: &mut Vec<f32>) {
        self.pcm.pop_samples(wanted, out);
    }

    /// Removes one front PCM sample if available.
    pub fn pop_pcm_front(&self) {
        self.pcm.pop_front();
    }

    /// Removes all queued PCM samples.
    pub fn clear_pcm_buffer(&self) {
        while self.pcm.len() > 0 {
            self.pcm.pop_front();
        }
    }

    /// Records that the host output callback ran.
    pub fn note_callback_activity(&self) {
        let _ = self.callback_heartbeat_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the last observed callback heartbeat epoch.
    pub fn callback_heartbeat_epoch(&self) -> u64 {
        self.callback_heartbeat_epoch.load(Ordering::Relaxed)
    }

    /// Returns buffered PCM sample count.
    pub fn pcm_len(&self) -> usize {
        self.pcm.len()
    }

    /// Returns PCM staging buffer capacity in samples.
    pub fn pcm_capacity_samples(&self) -> usize {
        self.pcm.capacity()
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
        self.pcm.underrun_samples()
    }

    /// Enables or disables PCM rendering for the current audio session.
    pub fn set_audio_output_enabled(&self, enabled: bool) {
        self.audio_output_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Returns whether PCM rendering is enabled for the active audio session.
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

    /// Configures buffering targets and soft-cap based on target latency.
    pub fn configure_buffering(&self, target_latency_samples: usize) {
        self.set_target_latency_samples(target_latency_samples);
        let staging_capacity = self.pcm_capacity_samples();
        if target_latency_samples < 128 {
            log::error!(
                "Audio config invalid: target_latency={} samples below minimum 128",
                target_latency_samples
            );
        }
        if staging_capacity < target_latency_samples.saturating_mul(2) {
            log::error!(
                "Audio config invalid: staging_capacity={} samples < target_latency*2 ({} samples)",
                staging_capacity,
                target_latency_samples.saturating_mul(2)
            );
        }
        if target_latency_samples > staging_capacity / 2 {
            log::error!(
                "Audio config invalid: target_latency={} samples exceeds staging_capacity/2 ({} samples)",
                target_latency_samples,
                staging_capacity / 2
            );
        }
        if staging_capacity <= target_latency_samples {
            log::error!(
                "Audio config invalid: staging_capacity={} samples <= target_latency={} samples",
                staging_capacity,
                target_latency_samples
            );
        }
        let soft_cap = compute_soft_cap_samples(target_latency_samples, staging_capacity);
        let min_cap = target_latency_samples.saturating_mul(2);
        let max_cap = staging_capacity.saturating_sub(target_latency_samples);
        if soft_cap < min_cap || soft_cap > max_cap {
            log::error!(
                "Audio config invalid: soft_cap={} samples not in [{}, {}]",
                soft_cap,
                min_cap,
                max_cap
            );
        }
        self.set_pcm_soft_cap_samples(soft_cap);
        log::info!(
            "Audio buffer policy: soft_cap={} samples (target_latency={}, staging_capacity={})",
            soft_cap,
            target_latency_samples,
            staging_capacity
        );
    }

    /// Returns the target latency in synth samples, or 0 if unset.
    pub fn target_latency_samples(&self) -> usize {
        self.target_latency_samples.load(Ordering::Relaxed) as usize
    }

    /// Adds dropped stale event count.
    pub fn add_dropped_stale_events(&self, count: u64) {
        self.dropped_stale_events.fetch_add(count, Ordering::Relaxed);
    }

    /// Returns elapsed time since last engine stats log if the interval passed.
    pub fn engine_stats_elapsed(&self, interval: Duration) -> Option<Duration> {
        let mut stats = self.stats.lock().unwrap();
        let elapsed = stats.last_engine_stats.elapsed();
        if elapsed >= interval {
            stats.last_engine_stats = Instant::now();
            Some(elapsed)
        } else {
            None
        }
    }

    /// Returns true when engine debug logs should be emitted.
    pub fn should_log_engine_debug(&self, interval: Duration) -> bool {
        let mut stats = self.stats.lock().unwrap();
        if stats.last_engine_debug.elapsed() >= interval {
            stats.last_engine_debug = Instant::now();
            true
        } else {
            false
        }
    }

    /// Returns true when postprocess logs should be emitted.
    pub fn should_log_postprocess(&self, interval: Duration) -> bool {
        let mut stats = self.stats.lock().unwrap();
        if stats.last_postprocess.elapsed() >= interval {
            stats.last_postprocess = Instant::now();
            true
        } else {
            false
        }
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
    readbacks: ReadbackResolver,
    drained_write_events: Vec<AudioEvent>,
    produced_samples: Vec<f32>,
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
    stats_samples: u64,
}

impl AudioEngine {
    /// Creates a new engine with timing parameters for CPU and synth domains.
    pub fn new(
        runtime: SharedAudioDataPlane,
        control_queues: SharedAudioControlPlane,
        config: AudioConfig,
    ) -> Self {
        let timing = AudioTiming::new(config);
        let history_horizon_cycles = (timing.cpu_hz as usize).max(8192);

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
            readbacks: ReadbackResolver::new(history_horizon_cycles),
            drained_write_events: Vec::new(),
            produced_samples: Vec::new(),
            synth_sample_rate: timing.synth_sample_rate,
            cpu_cycles_per_sample: timing.cpu_cycles_per_sample,
            rendered_cpu_cycle: 0.0,
            target_latency_cycles: timing.target_latency_cycles.max(0.0),
            history_horizon_cycles,
            max_pending_writes: 8192,
            snapshot_dirty_since_capture: false,
            stats: PcmStats {
                stats_samples: 0,
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

    /// Updates CPU timing when the emulator CPU speed diverges from the nominal clock.
    pub fn update_cpu_hz(&mut self, cpu_hz: f64) {
        let new_cpu_hz = cpu_hz.max(1.0);
        let current_cpu_hz = self.cpu_cycles_per_sample * self.synth_sample_rate as f64;
        let rel_delta = (new_cpu_hz - current_cpu_hz).abs() / current_cpu_hz.max(1.0);
        if rel_delta < 0.02 {
            return;
        }

        self.cpu_cycles_per_sample = new_cpu_hz / self.synth_sample_rate as f64;
        let target_latency_samples = self.runtime.target_latency_samples().max(1);
        self.target_latency_cycles = target_latency_samples as f64 * self.cpu_cycles_per_sample;
        self.history_horizon_cycles = (new_cpu_hz as usize).max(8192);
        self.readbacks.set_history_horizon(self.history_horizon_cycles);

        log::trace!(
            "Audio engine retuned: cpu_hz={:.2}, cycles_per_sample={:.3}, target_latency_cycles={:.1} (~{} samples)",
            new_cpu_hz,
            self.cpu_cycles_per_sample,
            self.target_latency_cycles,
            target_latency_samples
        );
    }

    /// Advances synthesis toward `cpu_cycle` while maintaining target latency.
    pub fn advance_to_cpu_cycle(&mut self, cpu_cycle: usize) {
        self.last_cpu_cycle = cpu_cycle;
        let target_cycle = (cpu_cycle as f64 - self.target_latency_cycles).max(0.0);
        self.advance_to_target_cycle(target_cycle);
    }

    /// Resolves a readback without advancing the main audio timeline.
    pub fn resolve_readback_value(&mut self, cycle: usize, register: u8, max_cycle: usize) -> u8 {
        self.drain_write_events();
        let ctx = ReadbackContext {
            synth: &self.synth,
            pending_writes: &self.pending_writes,
            rendered_cpu_cycle: self.rendered_cpu_cycle,
            cpu_cycles_per_sample: self.cpu_cycles_per_sample,
            synth_sample_rate: self.synth_sample_rate,
        };
        self.readbacks
            .resolve_readback_value(cycle, register, max_cycle, ctx)
    }

    /// Drains pending write events into the local queue in FIFO order.
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

    /// Returns true if a debug log should be emitted this tick.
    fn should_log_debug(&mut self, now: Instant) -> bool {
        let _ = now;
        self.runtime
            .should_log_engine_debug(Duration::from_secs(1))
    }

    /// Clamps the target cycle based on PCM buffering headroom.
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

    /// Applies any queued writes scheduled before the current sample cycle.
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

    /// Applies queued writes up to and including the requested cycle.
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

    /// Drops stale pending writes that fall outside the history horizon.
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

    /// Pushes produced samples into the shared PCM buffer and updates snapshot.
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
                self.readbacks.clear_batch();
                self.runtime.store_snapshot(self.synth.clone());
                self.snapshot_dirty_since_capture = false;
                log::info!("Audio engine session resumed at cycle {:.1}", self.rendered_cpu_cycle);
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
        self.readbacks.clear_batch();

        let remaining_cycles = (target_cycle - self.rendered_cpu_cycle).max(0.0);
        let estimated_steps = (remaining_cycles / self.cpu_cycles_per_sample).ceil() as usize;
        if estimated_steps > 0 && log_ready {
            log::debug!(
                "Audio engine: advancing {} samples (remaining_cycles={:.1})",
                estimated_steps,
                remaining_cycles
            );
        }
        self.produced_samples.reserve(estimated_steps);
        self.readbacks.reserve_batch(estimated_steps);

        while self.rendered_cpu_cycle + self.cpu_cycles_per_sample <= target_cycle {
            let sample_cycle = self.rendered_cpu_cycle as usize;
            self.apply_pending_writes(sample_cycle);

            let sample = self.synth.render_sample(self.synth_sample_rate);
            self.produced_samples.push(sample);

            self.readbacks.push_readback(sample_cycle, &self.synth);

            self.rendered_cpu_cycle += self.cpu_cycles_per_sample;
        }

        if !self.produced_samples.is_empty() {
            self.stats.stats_samples = self
                .stats.stats_samples
                .saturating_add(self.produced_samples.len() as u64);
        }
        self.readbacks.flush(self.rendered_cpu_cycle);
        let dropped_stale = self.drop_stale_writes();

        if self.produced_samples.is_empty() && dropped_stale == 0 {
            return;
        }

        let sample_count = self.flush_produced_samples();
        if sample_count > 0 {
            log::debug!(
                "Audio engine step: {} samples produced, total queued PCM: {}",
                sample_count,
                self.runtime.pcm_len()
            );
        }

        if let Some(stats_elapsed) = self.runtime.engine_stats_elapsed(Duration::from_secs(1)) {
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
