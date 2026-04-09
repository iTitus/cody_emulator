//! Optional CPAL-backed audio output host.
//!
//! This module consumes postprocessed audio and feeds it into the platform
//! output stream when the `cpal-backend` feature is enabled.

use crate::device::audio::engine::SharedAudioDataPlane;
use crate::device::audio::postprocess::{
    AudioEffectChain, AudioPostProcessConfig, AudioPostProcessor, PostResamplerFactory,
};
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
    Callback(u64),
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
    fn note_callback(&self) {
        self.with_state(|state| {
            state.callback_epoch = state.callback_epoch.wrapping_add(1);
        });
        self.cv.notify_one();
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
            return MonitorWait::Callback(guard.callback_epoch);
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
            MonitorWait::Callback(guard.callback_epoch)
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
                MonitorWait::Callback(next_epoch) => {
                    callback_epoch = next_epoch;
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
                    MonitorWait::Callback(next_epoch) => {
                        state.last_callback_epoch = next_epoch;
                        state.no_callback_seconds = 0;
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

        let mut stream_cfg = default_cfg.config();
        let sample_format = default_cfg.sample_format();
        let preferred_min = config.preferred_output_buffer_frames.max(128);
        if let Some(buffer_size) = Self::pick_buffer_size(
            &device,
            sample_format,
            stream_cfg.channels,
            stream_cfg.sample_rate.0,
            preferred_min,
        ) {
            stream_cfg.buffer_size = buffer_size;
            log::info!("CPAL: using buffer_size={buffer_size:?}");
        } else {
            log::info!(
                "CPAL: using default buffer_size={:?}",
                stream_cfg.buffer_size
            );
        }
        config.sample_rate_hz = stream_cfg.sample_rate.0;
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
                stream_cfg.sample_rate.0,
                stream_cfg.channels,
                stream_cfg.buffer_size
            );
        };
        let err_fn = |err| log::warn!("CPAL output stream error: {err}");

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
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [f32], _| {
                        callback_runtime.note_callback_activity();
                        callback_signals.note_callback();
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
            SampleFormat::I16 => {
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
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [i16], _| {
                        callback_runtime.note_callback_activity();
                        callback_signals.note_callback();
                        Self::fill_output_i16(
                            data,
                            channels,
                            &mut post_processor,
                            &config,
                            &mut scratch,
                        );
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
            SampleFormat::U16 => {
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
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [u16], _| {
                        callback_runtime.note_callback_activity();
                        callback_signals.note_callback();
                        Self::fill_output_u16(
                            data,
                            channels,
                            &mut post_processor,
                            &config,
                            &mut scratch,
                        );
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
            _ => {
                log::warn!("CPAL sample_format={sample_format:?} not supported");
                None
            }
        }
    }

    #[cfg(feature = "cpal-backend")]
    /// Logs the negotiated output format and estimated end-to-end latency.
    fn log_output_config(
        stream_cfg: &cpal::StreamConfig,
        sample_format: cpal::SampleFormat,
        runtime: &SharedAudioDataPlane,
        synth_sample_rate: u32,
    ) {
        let output_rate = stream_cfg.sample_rate.0.max(1) as f64;
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

        log::info!(
            "CPAL output config: rate={}Hz, channels={}, format={sample_format:?}, buffer_size={:?}, output_latency_ms={}, synth_lag_ms={}, approx_end_to_end_ms={}",
            stream_cfg.sample_rate.0,
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
                .unwrap_or_else(|| "unknown".to_string())
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
        };

        thread::spawn(move || monitor.run(ready_tx))
    }

    #[cfg(feature = "cpal-backend")]
    /// Picks a buffer size aligned to typical audio frame boundaries.
    fn pick_buffer_size(
        device: &cpal::Device,
        sample_format: cpal::SampleFormat,
        channels: u16,
        sample_rate: u32,
        min_inclusive: u32,
    ) -> Option<cpal::BufferSize> {
        use cpal::traits::DeviceTrait;
        const MIN_ALIGNED_FRAMES: u32 = 128;
        const ALIGN_FRAMES: u32 = 64;
        let mut best: Option<u32> = None;

        let Ok(configs) = device.supported_output_configs() else {
            return None;
        };

        for cfg in configs {
            if cfg.sample_format() != sample_format {
                continue;
            }
            if cfg.channels() != channels {
                continue;
            }
            let sample_rate_range = cfg.min_sample_rate().0..=cfg.max_sample_rate().0;
            if !sample_rate_range.contains(&sample_rate) {
                continue;
            }
            match cfg.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => {
                    let mut candidate = min_inclusive.max(*min);
                    candidate = candidate.max(MIN_ALIGNED_FRAMES);
                    let aligned = candidate.saturating_add(ALIGN_FRAMES - 1) / ALIGN_FRAMES;
                    let aligned = aligned.saturating_mul(ALIGN_FRAMES);
                    if aligned <= *max {
                        best = match best {
                            Some(current) if current <= aligned => Some(current),
                            _ => Some(aligned),
                        };
                    }
                }
                cpal::SupportedBufferSize::Unknown => {}
            }
        }

        best.map(cpal::BufferSize::Fixed)
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

    /// Fills an i16 output buffer from postprocessed samples.
    #[cfg(feature = "cpal-backend")]
    fn fill_output_i16(
        data: &mut [i16],
        channels: usize,
        post_processor: &mut AudioPostProcessor,
        config: &AudioPostProcessConfig,
        scratch: &mut Vec<f32>,
    ) {
        let _ = channels;
        scratch.resize(data.len(), 0.0);
        post_processor.render_interleaved(scratch, config);
        post_processor.print_debug_stuff(false);
        for (dst, sample) in data.iter_mut().zip(scratch.iter()) {
            let s = sample.clamp(-1.0, 1.0);
            *dst = (s * i16::MAX as f32) as i16;
        }
    }

    /// Fills a u16 output buffer from postprocessed samples.
    #[cfg(feature = "cpal-backend")]
    fn fill_output_u16(
        data: &mut [u16],
        channels: usize,
        post_processor: &mut AudioPostProcessor,
        config: &AudioPostProcessConfig,
        scratch: &mut Vec<f32>,
    ) {
        let _ = channels;
        scratch.resize(data.len(), 0.0);
        post_processor.render_interleaved(scratch, config);
        post_processor.print_debug_stuff(false);
        for (dst, sample) in data.iter_mut().zip(scratch.iter()) {
            let s = sample.clamp(-1.0, 1.0);
            let mapped = (s * 0.5 + 0.5) * u16::MAX as f32;
            *dst = mapped.round() as u16;
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
