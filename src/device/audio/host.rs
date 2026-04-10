//! Optional CPAL-backed audio output host.
//!
//! This module consumes postprocessed audio and feeds it into the platform
//! output stream when the `cpal-backend` feature is enabled.

use crate::device::audio::engine::SharedAudioDataPlane;
use crate::device::audio::postprocess::{
    AudioEffectChain, AudioPostProcessConfig, AudioPostProcessor, PostResamplerFactory,
};
use crate::device::audio::compute_soft_cap_samples;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

/// Backend-free host that drains synth PCM without opening an output device.
///
/// This disables backend output entirely while intentionally skipping the
/// postprocess render path (including resampling and output effects).
pub struct StubHost;

impl StubHost {
    /// Creates a no-backend host used when audio output is explicitly disabled.
    pub fn new(post_processor: AudioPostProcessor) -> Self {
        let runtime = post_processor.shared_data_plane();
        runtime.set_audio_output_enabled(false);

        log::info!(
            "Audio output disabled; using a stub host. The audio engine is not disabled by this."
        );

        Self
    }

    /// Stub host is ready immediately because no external device is required.
    pub fn wait_ready(&mut self, _timeout: Duration) -> bool {
        true
    }
}

/// CPAL host wrapper that owns output stream lifetime.
pub struct CpalHost {
    #[cfg(not(feature = "cpal-backend"))]
    _post_processor: AudioPostProcessor,
    #[cfg(feature = "cpal-backend")]
    _monitor: Option<std::thread::JoinHandle<()>>,
    #[cfg(feature = "cpal-backend")]
    ready_rx: Option<mpsc::Receiver<()>>,
    #[cfg(feature = "cpal-backend")]
    signals: Arc<MonitorSignals>,
}

#[cfg(feature = "cpal-backend")]
struct AdaptiveMonitor {
    device: cpal::Device,
    sample_format: cpal::SampleFormat,
    stream_cfg: cpal::StreamConfig,
    config: AudioPostProcessConfig,
    runtime: SharedAudioDataPlane,
    synth_sample_rate: u32,
    effects: AudioEffectChain,
    initial_catchup_enabled: bool,
    resampler_factory: PostResamplerFactory,
    signals: Arc<MonitorSignals>,
    baseline_target_latency_samples: usize,
    baseline_soft_cap_samples: usize,
    callback_ema_synth_samples_q10: u64,
}

#[cfg(feature = "cpal-backend")]
struct MonitorState {
    last_callback_epoch: u64,
    no_callback_seconds: u64,
}

#[cfg(feature = "cpal-backend")]
#[derive(Default)]
/// Condvar-protected monitor state.
///
/// `callback_epoch` increments for each host callback and is used as a monotonic
/// sequence number so the monitor can wait for "next callback" events.
struct MonitorSignalsState {
    shutdown: bool,
    callback_epoch: u64,
    callback_last_synth_samples: usize,
    callback_peak_synth_samples: usize,
}

#[cfg(feature = "cpal-backend")]
/// Synchronization hub for CPAL callbacks and the adaptive monitor thread.
///
/// This keeps monitor waits event-driven (callback or shutdown) while still
/// allowing bounded timeout waits for restart/backoff policies.
struct MonitorSignals {
    state: Mutex<MonitorSignalsState>,
    cv: Condvar,
}

#[cfg(feature = "cpal-backend")]
/// Outcome of waiting for monitor progress.
enum MonitorWait {
    Callback {
        epoch: u64,
        synth_samples: usize,
    },
    Shutdown,
    Timeout,
}

#[cfg(feature = "cpal-backend")]
impl MonitorSignals {
    /// Creates an empty signal set with no callbacks and no shutdown request.
    fn new() -> Self {
        Self {
            state: Mutex::new(MonitorSignalsState::default()),
            cv: Condvar::new(),
        }
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut MonitorSignalsState) -> T) -> T {
        let mut guard = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        f(&mut guard)
    }

    /// Returns true when host shutdown has been requested.
    fn stop_requested(&self) -> bool {
        self.with_state(|state| state.shutdown)
    }

    /// Requests monitor shutdown and wakes all waiters.
    fn request_shutdown(&self) {
        self.with_state(|state| {
            state.shutdown = true;
        });
        self.cv.notify_all();
    }

    /// Reads the current callback epoch.
    fn current_callback_epoch(&self) -> u64 {
        self.with_state(|state| state.callback_epoch)
    }

    /// Publishes one callback event and wakes a waiting monitor.
    fn note_callback_synth_samples(&self, synth_samples: usize) {
        self.with_state(|state| {
            state.callback_epoch = state.callback_epoch.wrapping_add(1);
            state.callback_last_synth_samples = synth_samples;
            if synth_samples > state.callback_peak_synth_samples {
                state.callback_peak_synth_samples = synth_samples;
            }
        });
        self.cv.notify_one();
    }

    /// Returns and resets callback synth-sample stats accumulated since the previous call.
    fn take_callback_synth_sample_stats(&self) -> (usize, usize) {
        self.with_state(|state| {
            let last = state.callback_last_synth_samples;
            let peak = state.callback_peak_synth_samples;
            state.callback_peak_synth_samples = 0;
            (last, peak)
        })
    }

    /// Waits for shutdown for up to `timeout` and returns whether shutdown was requested.
    fn wait_for_shutdown_or_timeout(&self, timeout: Duration) -> bool {
        let guard = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if guard.shutdown {
            return true;
        }

        let (guard, _) = self
            .cv
            .wait_timeout_while(guard, timeout, |state| !state.shutdown)
            .unwrap_or_else(|poison| poison.into_inner());
        guard.shutdown
    }

    /// Waits for either a callback newer than `since_epoch`, or a shutdown request.
    ///
    /// Returns `Timeout` when neither event is observed before `timeout`.
    fn wait_for_callback_or_shutdown(&self, since_epoch: u64, timeout: Duration) -> MonitorWait {
        let guard = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if guard.shutdown {
            return MonitorWait::Shutdown;
        }
        if guard.callback_epoch != since_epoch {
            return MonitorWait::Callback {
                epoch: guard.callback_epoch,
                synth_samples: guard.callback_last_synth_samples,
            };
        }

        let (guard, timeout_result) = self
            .cv
            .wait_timeout_while(guard, timeout, |state| {
                !state.shutdown && state.callback_epoch == since_epoch
            })
            .unwrap_or_else(|poison| poison.into_inner());

        if guard.shutdown {
            MonitorWait::Shutdown
        } else if guard.callback_epoch != since_epoch {
            MonitorWait::Callback {
                epoch: guard.callback_epoch,
                synth_samples: guard.callback_last_synth_samples,
            }
        } else if timeout_result.timed_out() {
            MonitorWait::Timeout
        } else {
            MonitorWait::Timeout
        }
    }
}

#[cfg(feature = "cpal-backend")]
/// State for the adaptive monitor thread, which restarts the CPAL stream when callbacks stop.
impl AdaptiveMonitor {
    const FIRST_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5);
    const NO_CALLBACK_SECS_THRESHOLD: u64 = 3;
    const RESTART_COOLDOWN: Duration = Duration::from_secs(5);
    const BASELINE_CALLBACK_OUTPUT_FRAMES: usize = 256;

    fn mul_div_round(value: usize, mul: usize, div: usize) -> usize {
        if div == 0 {
            return value;
        }
        let num = value as u128 * mul as u128;
        ((num + (div as u128 / 2)) / div as u128) as usize
    }

    fn align_up(value: usize, align: usize) -> usize {
        if align <= 1 {
            return value;
        }
        value.div_ceil(align) * align
    }

    /// Retunes runtime buffering from observed callback size in synth PCM samples.
    fn adapt_buffering_from_callback_synth_samples(&mut self, synth_samples: usize) {
        const EMA_WEIGHT_OLD: u64 = 9;
        const EMA_WEIGHT_NEW: u64 = 1;
        const EMA_WEIGHT_TOTAL: u64 = EMA_WEIGHT_OLD + EMA_WEIGHT_NEW;
        const TARGET_ALIGN: usize = 8;
        const TARGET_UPDATE_MIN_DELTA: usize = 8;

        let observed_synth = synth_samples;
        if observed_synth == 0 {
            return;
        }

        let observed_q10 = (observed_synth as u64).saturating_mul(1024);
        let ema_q10 = if self.callback_ema_synth_samples_q10 == 0 {
            observed_q10
        } else {
            (self
                .callback_ema_synth_samples_q10
                .saturating_mul(EMA_WEIGHT_OLD)
                .saturating_add(observed_q10.saturating_mul(EMA_WEIGHT_NEW)))
                / EMA_WEIGHT_TOTAL
        };
        self.callback_ema_synth_samples_q10 = ema_q10;

        let ema_synth = ((ema_q10 + 512) / 1024) as usize;
        let effective_synth = observed_synth.max(ema_synth).max(1);
        self.runtime
            .set_callback_effective_synth_samples(effective_synth);

        let baseline_synth = CpalHost::output_frames_to_synth_samples(
            Self::BASELINE_CALLBACK_OUTPUT_FRAMES,
            self.stream_cfg.sample_rate,
            self.synth_sample_rate,
        )
        .max(1);

        let staging_capacity = self.runtime.pcm_capacity_samples();
        if staging_capacity <= 1 {
            return;
        }

        let mut target = Self::mul_div_round(
            self.baseline_target_latency_samples,
            effective_synth,
            baseline_synth,
        );
        let latency_bias_q10 = self.runtime.latency_bias_q10().clamp(768, 1536) as usize;
        target = Self::mul_div_round(target, latency_bias_q10, 1024);
        let min_target = (self.baseline_target_latency_samples.saturating_mul(3) / 4).max(64);
        let max_target = (staging_capacity / 3).max(min_target);
        target = target.clamp(min_target, max_target);
        target = Self::align_up(target, TARGET_ALIGN);

        let current_target = self.runtime.target_latency_samples();
        let target_changed = current_target.abs_diff(target) >= TARGET_UPDATE_MIN_DELTA;
        if target_changed {
            self.runtime.set_target_latency_samples(target);
        } else {
            target = current_target.max(1);
        }

        let mut soft_cap =
            Self::mul_div_round(self.baseline_soft_cap_samples, effective_synth, baseline_synth);
        let min_cap = target.saturating_mul(2);
        let max_cap = staging_capacity.saturating_sub(target).max(min_cap);
        soft_cap = soft_cap.clamp(min_cap, max_cap);
        self.runtime.set_pcm_soft_cap_samples(soft_cap);
    }

    /// Tries to build one audio output session, retrying once with default buffer size.
    fn launch_session(&mut self) -> Option<cpal::Stream> {
        let mut stream = CpalHost::build_stream(
            &self.device,
            self.sample_format,
            &self.stream_cfg,
            &self.config,
            self.runtime.clone(),
            self.synth_sample_rate,
            self.config.channels.max(1),
            &self.effects,
            self.initial_catchup_enabled,
            self.resampler_factory,
            Arc::clone(&self.signals),
        );
        if stream.is_none() && !matches!(self.stream_cfg.buffer_size, cpal::BufferSize::Default) {
            log::warn!("CPAL: retrying with default buffer_size");
            self.stream_cfg.buffer_size = cpal::BufferSize::Default;
            stream = CpalHost::build_stream(
                &self.device,
                self.sample_format,
                &self.stream_cfg,
                &self.config,
                self.runtime.clone(),
                self.synth_sample_rate,
                self.config.channels.max(1),
                &self.effects,
                self.initial_catchup_enabled,
                self.resampler_factory,
                Arc::clone(&self.signals),
            );
        }
        stream
    }

    /// Runs the callback-gated session monitor loop and restarts streams when callbacks stop.
    fn run(mut self, ready_tx: mpsc::Sender<()>) {
        use cpal::traits::StreamTrait;

        let mut first_session_ready = false;

        loop {
            if self.signals.stop_requested() {
                log::info!("CPAL: stop requested before starting session");
                break;
            }

            // Session startup phase.
            self.runtime.set_audio_output_enabled(false);
            self.runtime.clear_pcm_buffer();
            log::info!("CPAL: starting audio session attempt");

            let stream = match self.launch_session() {
                Some(stream) => stream,
                None => {
                    log::info!(
                        "CPAL: restart attempt deferred while building session; cooling down for {:?}",
                        Self::RESTART_COOLDOWN
                    );
                    if self.signals.wait_for_shutdown_or_timeout(Self::RESTART_COOLDOWN) {
                        break;
                    }
                    continue;
                }
            };

            let mut callback_epoch = self.signals.current_callback_epoch();

            if let Err(err) = stream.play() {
                log::warn!("Unable to start CPAL output stream: {err}");
                log::info!(
                    "CPAL: restart attempt deferred after play() failure; cooling down for {:?}",
                    Self::RESTART_COOLDOWN
                );
                if self.signals.wait_for_shutdown_or_timeout(Self::RESTART_COOLDOWN) {
                    break;
                }
                continue;
            }

            log::info!(
                "CPAL: waiting for first callback (timeout {:?})",
                Self::FIRST_CALLBACK_TIMEOUT
            );

            match self
                .signals
                .wait_for_callback_or_shutdown(callback_epoch, Self::FIRST_CALLBACK_TIMEOUT)
            {
                MonitorWait::Callback {
                    epoch: next_epoch,
                    synth_samples,
                } => {
                    callback_epoch = next_epoch;
                    self.adapt_buffering_from_callback_synth_samples(synth_samples);
                    log::info!("CPAL: first host callback received");
                }
                MonitorWait::Shutdown => {
                    log::info!("CPAL: stop requested while waiting for first callback");
                    self.runtime.set_audio_output_enabled(false);
                    drop(stream);
                    break;
                }
                MonitorWait::Timeout => {
                    log::warn!(
                        "CPAL: no callback received within {:?}; restarting session",
                        Self::FIRST_CALLBACK_TIMEOUT
                    );
                    drop(stream);
                    log::info!(
                        "CPAL: restart attempt deferred after missing first callback; cooling down for {:?}",
                        Self::RESTART_COOLDOWN
                    );
                    if self.signals.wait_for_shutdown_or_timeout(Self::RESTART_COOLDOWN) {
                        break;
                    }
                    continue;
                }
            }

            // Session active phase: monitor callback heartbeats until stall or shutdown.
            self.runtime.set_audio_output_enabled(true);
            CpalHost::update_soft_cap(&self.runtime);
            CpalHost::log_output_config(
                &self.stream_cfg,
                self.sample_format,
                &self.runtime,
                self.synth_sample_rate,
            );
            if !first_session_ready {
                let _ = ready_tx.send(());
                first_session_ready = true;
            }

            let mut state = MonitorState {
                last_callback_epoch: callback_epoch,
                no_callback_seconds: 0,
            };

            let mut shutdown_now = false;

            loop {
                match self
                    .signals
                    .wait_for_callback_or_shutdown(state.last_callback_epoch, Duration::from_secs(1))
                {
                    MonitorWait::Callback {
                        epoch: next_epoch,
                        synth_samples,
                    } => {
                        state.last_callback_epoch = next_epoch;
                        state.no_callback_seconds = 0;
                        self.adapt_buffering_from_callback_synth_samples(synth_samples);
                    }
                    MonitorWait::Shutdown => {
                        shutdown_now = true;
                        break;
                    }
                    MonitorWait::Timeout => {
                        state.no_callback_seconds = state.no_callback_seconds.saturating_add(1);
                        if state.no_callback_seconds >= Self::NO_CALLBACK_SECS_THRESHOLD {
                            log::warn!(
                                "CPAL output callback stopped for {}s; restarting audio session",
                                state.no_callback_seconds
                            );
                            log::info!(
                                "CPAL: restart attempt deferred after callback stall; cooling down for {:?}",
                                Self::RESTART_COOLDOWN
                            );
                            break;
                        }
                    }
                }
            }

            self.runtime.set_audio_output_enabled(false);
            let (last_synth_samples, peak_synth_samples) =
                self.signals.take_callback_synth_sample_stats();
            let runtime_last_synth_samples = self.runtime.callback_last_buffer_synth_samples();
            let runtime_peak_synth_samples = self.runtime.take_callback_peak_buffer_synth_samples();
            let runtime_effective_synth_samples = self.runtime.callback_effective_synth_samples();
            if peak_synth_samples > 0 || runtime_peak_synth_samples > 0 {
                log::info!(
                    "CPAL callback bookkeeping stats: monitor_last_synth_samples={}, monitor_peak_synth_samples={}, runtime_last_synth_samples={}, runtime_peak_synth_samples={}, runtime_effective_synth_samples={}, target_latency_samples={}, soft_cap_samples={}",
                    last_synth_samples,
                    peak_synth_samples,
                    runtime_last_synth_samples,
                    runtime_peak_synth_samples,
                    runtime_effective_synth_samples,
                    self.runtime.target_latency_samples(),
                    self.runtime.pcm_soft_cap_samples()
                );
                log::info!(
                    "CPAL engine handoff stats: callback_effective_synth_samples={}, target_latency_samples={}, pcm_soft_cap_samples={}, latency_bias_q10={}, catchup_trigger_strictness_q10={}",
                    runtime_effective_synth_samples,
                    self.runtime.target_latency_samples(),
                    self.runtime.pcm_soft_cap_samples(),
                    self.runtime.latency_bias_q10(),
                    self.runtime.catchup_trigger_strictness_q10()
                );
            }
            drop(stream);

            if shutdown_now {
                break;
            }

            log::info!(
                "CPAL: audio session closed; cooling down for {:?}",
                Self::RESTART_COOLDOWN
            );
            if self.signals.wait_for_shutdown_or_timeout(Self::RESTART_COOLDOWN) {
                break;
            }
        }

        self.runtime.set_audio_output_enabled(false);
        log::debug!("CPAL: monitor thread exiting");
    }
}

impl CpalHost {
    #[cfg(feature = "cpal-backend")]
    fn output_frames_to_synth_samples(
        output_frames: usize,
        output_sample_rate_hz: u32,
        synth_sample_rate_hz: u32,
    ) -> usize {
        if output_frames == 0 || output_sample_rate_hz == 0 || synth_sample_rate_hz == 0 {
            return 0;
        }
        let num = output_frames as u128 * synth_sample_rate_hz as u128;
        let den = output_sample_rate_hz as u128;
        num.div_ceil(den) as usize
    }

    #[cfg(feature = "cpal-backend")]
    /// Returns whether this host implementation can render directly to `sample_format`.
    fn supports_sample_format(sample_format: cpal::SampleFormat) -> bool {
        use cpal::SampleFormat;

        matches!(
            sample_format,
            SampleFormat::F32
                | SampleFormat::F64
                | SampleFormat::I8
                | SampleFormat::I16
                | SampleFormat::I24
                | SampleFormat::I32
                | SampleFormat::I64
                | SampleFormat::U8
                | SampleFormat::U16
                | SampleFormat::U24
                | SampleFormat::U32
                | SampleFormat::U64
        )
    }

    #[cfg(feature = "cpal-backend")]
    /// Selects a compatible output stream configuration from device capabilities.
    ///
    /// Preference order:
    /// 1) Device default config if format is supported.
    /// 2) Best supported config (CPAL default heuristics) constrained to supported formats,
    ///    preferring the default sample rate when available.
    fn select_compatible_output_config(
        device: &cpal::Device,
        default_cfg: cpal::SupportedStreamConfig,
    ) -> Option<(cpal::StreamConfig, cpal::SampleFormat)> {
        use cpal::traits::DeviceTrait;

        let default_rate = default_cfg.sample_rate();
        let default_format = default_cfg.sample_format();
        if Self::supports_sample_format(default_format) {
            return Some((default_cfg.config(), default_format));
        }

        let mut configs: Vec<_> = device.supported_output_configs().ok()?.collect();

        // Prefer CPAL's default heuristics, but constrain to formats we implement.
        configs.sort_by(|a, b| b.cmp_default_heuristics(a));
        let selected = configs.into_iter().find_map(|cfg| {
            if !Self::supports_sample_format(cfg.sample_format()) {
                return None;
            }

            let min_rate = cfg.min_sample_rate();
            let max_rate = cfg.max_sample_rate();
            if (min_rate..=max_rate).contains(&default_rate) {
                Some(cfg.with_sample_rate(default_rate))
            } else {
                Some(cfg.with_max_sample_rate())
            }
        })?;

        Some((selected.config(), selected.sample_format()))
    }

    #[cfg(feature = "cpal-backend")]
    /// Updates soft-cap buffering based on the current target latency.
    fn update_soft_cap(runtime: &SharedAudioDataPlane) {
        let target_latency_samples = runtime.target_latency_samples();
        runtime.configure_buffering(target_latency_samples);
    }

    /// Creates a host and starts the output stream when CPAL is enabled.
    pub fn new(post_processor: AudioPostProcessor, config: AudioPostProcessConfig) -> Self {
        #[cfg(feature = "cpal-backend")]
        let (ready_tx, ready_rx) = mpsc::channel();
        #[cfg(feature = "cpal-backend")]
        let signals = Arc::new(MonitorSignals::new());
        #[cfg(feature = "cpal-backend")]
        let monitor = Self::start_stream(post_processor, config, ready_tx, Arc::clone(&signals));

        #[cfg(not(feature = "cpal-backend"))]
        {
            let _ = config;
            log::info!(
                "CPAL backend disabled; build with feature 'cpal-backend' to enable audio output stream"
            );
            return Self {
                _post_processor: post_processor,
            };
        }

        #[cfg(feature = "cpal-backend")]
        {
            return Self {
                _monitor: monitor,
                ready_rx: Some(ready_rx),
                signals,
            };
        }
    }

    /// Waits for the audio stream to report readiness.
    #[cfg(feature = "cpal-backend")]
    pub fn wait_ready(&mut self, timeout: Duration) -> bool {
        let Some(rx) = self.ready_rx.take() else {
            return true;
        };
        rx.recv_timeout(timeout).is_ok()
    }

    /// No-op readiness wait when CPAL is disabled.
    #[cfg(not(feature = "cpal-backend"))]
    pub fn wait_ready(&mut self, _timeout: Duration) -> bool {
        true
    }

    #[cfg(feature = "cpal-backend")]
    /// Builds and launches the CPAL output stream and monitor thread.
    fn start_stream(
        post_processor: AudioPostProcessor,
        mut config: AudioPostProcessConfig,
        ready_tx: mpsc::Sender<()>,
        signals: Arc<MonitorSignals>,
    ) -> Option<std::thread::JoinHandle<()>> {
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            log::warn!("No default output device available for CPAL audio host");
            return None;
        };

        let Ok(default_cfg) = device.default_output_config() else {
            log::warn!("Unable to get default output config from CPAL device");
            return None;
        };

        let default_format = default_cfg.sample_format();
        let Some((stream_cfg, sample_format)) =
            Self::select_compatible_output_config(&device, default_cfg)
        else {
            log::warn!(
                "CPAL default sample format {default_format:?} is unsupported and no compatible fallback format was found"
            );
            return None;
        };

        if sample_format != default_format {
            log::info!(
                "CPAL: selected fallback sample format={sample_format:?} rate={}Hz channels={} when default format was unsupported",
                stream_cfg.sample_rate,
                stream_cfg.channels
            );
        }
        log::info!(
            "CPAL: keeping device default buffer_size={:?}",
            stream_cfg.buffer_size
        );
        config.sample_rate_hz = stream_cfg.sample_rate;
        config.channels = stream_cfg.channels as usize;

        let runtime = post_processor.shared_data_plane();
        let synth_sample_rate = post_processor.synth_sample_rate();
        let effects = post_processor.effect_chain();
        let initial_catchup_enabled = post_processor.initial_catchup_enabled();
        let resampler_factory = post_processor.resampler_factory();
        Self::update_soft_cap(&runtime);
        let monitor = Self::spawn_adaptive_monitor(
            device,
            sample_format,
            stream_cfg,
            config,
            runtime,
            synth_sample_rate,
            effects,
            initial_catchup_enabled,
            resampler_factory,
            ready_tx,
            signals,
        );

        Some(monitor)
    }

    #[cfg(feature = "cpal-backend")]
    /// Builds and launches the CPAL output stream and monitor thread.
    fn build_post_processor(
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        effects: &AudioEffectChain,
        initial_catchup_enabled: bool,
        resampler_factory: PostResamplerFactory,
    ) -> AudioPostProcessor {
        let mut post_processor = AudioPostProcessor::new_with_resampler_factory(
            runtime,
            synth_sample_rate,
            resampler_factory,
        );
        post_processor.set_initial_catchup_enabled(initial_catchup_enabled);
        if !effects.pre.is_empty() {
            post_processor.set_pre_effects(effects.pre.clone());
        }
        if !effects.post.is_empty() {
            post_processor.set_post_effects(effects.post.clone());
        }
        post_processor
    }

    #[cfg(feature = "cpal-backend")]
    /// Creates a CPAL stream for the negotiated sample format.
    fn build_stream(
        device: &cpal::Device,
        sample_format: cpal::SampleFormat,
        stream_cfg: &cpal::StreamConfig,
        config: &AudioPostProcessConfig,
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        channels: usize,
        effects: &AudioEffectChain,
        initial_catchup_enabled: bool,
        resampler_factory: PostResamplerFactory,
        signals: Arc<MonitorSignals>,
    ) -> Option<cpal::Stream> {
        use cpal::SampleFormat;
        use cpal::traits::DeviceTrait;

        let log_build_error = |err: &cpal::BuildStreamError| {
            log::warn!(
                "Unable to build CPAL output stream: {err}; format={sample_format:?}, rate={}, channels={}, buffer_size={:?}",
                stream_cfg.sample_rate,
                stream_cfg.channels,
                stream_cfg.buffer_size
            );
        };

        match sample_format {
            SampleFormat::F32 => {
                let mut post_processor = Self::build_post_processor(
                    runtime,
                    synth_sample_rate,
                    effects,
                    initial_catchup_enabled,
                    resampler_factory,
                );
                let callback_runtime = post_processor.shared_data_plane();
                let callback_signals = Arc::clone(&signals);
                let config = config.clone();
                let output_sample_rate_hz = stream_cfg.sample_rate;
                let synth_sample_rate_hz = synth_sample_rate;
                let err_fn = |err| log::warn!("CPAL output stream error: {err}");
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [f32], _| {
                        let output_frames = data.len() / channels.max(1);
                        let synth_samples = CpalHost::output_frames_to_synth_samples(
                            output_frames,
                            output_sample_rate_hz,
                            synth_sample_rate_hz,
                        );
                        callback_runtime.note_callback_activity_with_synth_samples(synth_samples);
                        callback_signals.note_callback_synth_samples(synth_samples);
                        Self::fill_output_f32(data, channels, &mut post_processor, &config);
                    },
                    err_fn,
                    None,
                ) {
                    Ok(stream) => Some(stream),
                    Err(err) => {
                        log_build_error(&err);
                        None
                    }
                }
            }
            SampleFormat::F64 => Self::build_stream_typed::<f64>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::I8 => Self::build_stream_typed::<i8>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::I16 => Self::build_stream_typed::<i16>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::I24 => Self::build_stream_typed::<cpal::I24>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::I32 => Self::build_stream_typed::<i32>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::I64 => Self::build_stream_typed::<i64>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::U8 => Self::build_stream_typed::<u8>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::U16 => Self::build_stream_typed::<u16>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::U24 => Self::build_stream_typed::<cpal::U24>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::U32 => Self::build_stream_typed::<u32>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            SampleFormat::U64 => Self::build_stream_typed::<u64>(
                device,
                stream_cfg,
                config,
                runtime,
                synth_sample_rate,
                channels,
                effects,
                initial_catchup_enabled,
                resampler_factory,
                signals,
            )
            .map_err(|err| {
                log_build_error(&err);
                err
            })
            .ok(),
            _ => {
                log::warn!("CPAL sample_format={sample_format:?} not supported");
                None
            }
        }
    }

    #[cfg(feature = "cpal-backend")]
    /// Builds a typed CPAL output stream, converting rendered f32 output to `T`.
    fn build_stream_typed<T>(
        device: &cpal::Device,
        stream_cfg: &cpal::StreamConfig,
        config: &AudioPostProcessConfig,
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        channels: usize,
        effects: &AudioEffectChain,
        initial_catchup_enabled: bool,
        resampler_factory: PostResamplerFactory,
        signals: Arc<MonitorSignals>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        use cpal::traits::DeviceTrait;

        let mut post_processor = Self::build_post_processor(
            runtime,
            synth_sample_rate,
            effects,
            initial_catchup_enabled,
            resampler_factory,
        );
        let callback_runtime = post_processor.shared_data_plane();
        let callback_signals = Arc::clone(&signals);
        let config = config.clone();
        let mut scratch: Vec<f32> = Vec::new();
        let output_sample_rate_hz = stream_cfg.sample_rate;
        let synth_sample_rate_hz = synth_sample_rate;

        device.build_output_stream(
            stream_cfg,
            move |data: &mut [T], _| {
                let output_frames = data.len() / channels.max(1);
                let synth_samples = CpalHost::output_frames_to_synth_samples(
                    output_frames,
                    output_sample_rate_hz,
                    synth_sample_rate_hz,
                );
                callback_runtime.note_callback_activity_with_synth_samples(synth_samples);
                callback_signals.note_callback_synth_samples(synth_samples);
                Self::fill_output_typed(data, channels, &mut post_processor, &config, &mut scratch);
            },
            |err| log::warn!("CPAL output stream error: {err}"),
            None,
        )
    }

    #[cfg(feature = "cpal-backend")]
    /// Logs the negotiated output format and estimated end-to-end latency.
    fn log_output_config(
        stream_cfg: &cpal::StreamConfig,
        sample_format: cpal::SampleFormat,
        runtime: &SharedAudioDataPlane,
        synth_sample_rate: u32,
    ) {
        let output_rate = stream_cfg.sample_rate.max(1) as f64;
        let buffer_frames = match stream_cfg.buffer_size {
            cpal::BufferSize::Fixed(frames) => Some(frames),
            _ => None,
        };
        let output_latency_ms = buffer_frames.map(|frames| frames as f64 / output_rate * 1000.0);
        let target_latency_samples = runtime.target_latency_samples();
        let synth_latency_ms = if target_latency_samples > 0 {
            Some(target_latency_samples as f64 / synth_sample_rate.max(1) as f64 * 1000.0)
        } else {
            None
        };
        let end_to_end_ms = match (output_latency_ms, synth_latency_ms) {
            (Some(out), Some(synth)) => Some(out + synth),
            _ => None,
        };
        let effective_callback_synth_samples = runtime.callback_effective_synth_samples();
        let effective_callback_output_frames = if effective_callback_synth_samples > 0 {
            (effective_callback_synth_samples as u128 * stream_cfg.sample_rate as u128)
                .div_ceil(synth_sample_rate.max(1) as u128) as usize
        } else {
            0
        };
        let soft_cap_samples = runtime.pcm_soft_cap_samples();
        let buffered_samples = runtime.pcm_len();

        log::info!(
            "CPAL output config: rate={}Hz, channels={}, format={sample_format:?}, buffer_size={:?}, output_latency_ms={}, synth_lag_ms={}, approx_end_to_end_ms={}, effective_callback_output_frames={}, effective_callback_synth_samples={}, target_latency_samples={}, soft_cap_samples={}, buffered_samples={}",
            stream_cfg.sample_rate,
            stream_cfg.channels,
            stream_cfg.buffer_size,
            output_latency_ms
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "unknown".to_string()),
            synth_latency_ms
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "unknown".to_string()),
            end_to_end_ms
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "unknown".to_string()),
            effective_callback_output_frames,
            effective_callback_synth_samples,
            target_latency_samples,
            soft_cap_samples,
            buffered_samples
        );
    }

    #[cfg(feature = "cpal-backend")]
    /// Spawns the adaptive monitor thread.
    fn spawn_adaptive_monitor(
        device: cpal::Device,
        sample_format: cpal::SampleFormat,
        stream_cfg: cpal::StreamConfig,
        config: AudioPostProcessConfig,
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        effects: AudioEffectChain,
        initial_catchup_enabled: bool,
        resampler_factory: PostResamplerFactory,
        ready_tx: mpsc::Sender<()>,
        signals: Arc<MonitorSignals>,
    ) -> std::thread::JoinHandle<()> {
        let baseline_target_latency_samples = runtime.target_latency_samples().max(1);
        let baseline_soft_cap_samples = runtime.pcm_soft_cap_samples().max(
            compute_soft_cap_samples(
                baseline_target_latency_samples,
                runtime.pcm_capacity_samples(),
            ),
        );

        let monitor = AdaptiveMonitor {
            device,
            sample_format,
            stream_cfg,
            config,
            runtime,
            synth_sample_rate,
            effects,
            initial_catchup_enabled,
            resampler_factory,
            signals,
            baseline_target_latency_samples,
            baseline_soft_cap_samples,
            callback_ema_synth_samples_q10: 0,
        };

        thread::spawn(move || monitor.run(ready_tx))
    }

    /// Fills an f32 output buffer from postprocessed samples.
    #[cfg(feature = "cpal-backend")]
    fn fill_output_f32(
        data: &mut [f32],
        channels: usize,
        post_processor: &mut AudioPostProcessor,
        config: &AudioPostProcessConfig,
    ) {
        let _ = channels;
        post_processor.render_interleaved(data, config);
        post_processor.print_debug_stuff(false);
    }

    /// Fills a typed output buffer from postprocessed samples.
    #[cfg(feature = "cpal-backend")]
    fn fill_output_typed<T>(
        data: &mut [T],
        channels: usize,
        post_processor: &mut AudioPostProcessor,
        config: &AudioPostProcessConfig,
        scratch: &mut Vec<f32>,
    )
    where
        T: cpal::Sample + cpal::FromSample<f32>,
    {
        let _ = channels;
        scratch.resize(data.len(), 0.0);
        post_processor.render_interleaved(scratch, config);
        post_processor.print_debug_stuff(false);
        for (dst, sample) in data.iter_mut().zip(scratch.iter()) {
            let s = sample.clamp(-1.0, 1.0);
            *dst = <T as cpal::Sample>::from_sample(s);
        }
    }
}

impl Drop for CpalHost {
    fn drop(&mut self) {
        #[cfg(feature = "cpal-backend")]
        {
            self.signals.request_shutdown();
            if let Some(monitor) = self._monitor.take() {
                monitor.thread().unpark();
                let _ = monitor.join();
            }
        }
    }
}
