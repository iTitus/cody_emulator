//! Optional CPAL-backed audio output host.
//!
//! This module consumes postprocessed audio and feeds it into the platform
//! output stream when the `cpal-backend` feature is enabled.

#[cfg(feature = "cpal-backend")]
use crate::device::audio::compute_soft_cap_samples;
use crate::device::audio::engine::SharedAudioDataPlane;
use crate::device::audio::postprocess::{AudioPostProcessConfig, AudioPostProcessor};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// CPAL host wrapper that owns output stream lifetime.
pub struct CpalHost {
    #[cfg(not(feature = "cpal-backend"))]
    _post_processor: AudioPostProcessor,
    #[cfg(feature = "cpal-backend")]
    _monitor: Option<std::thread::JoinHandle<()>>,
    #[cfg(feature = "cpal-backend")]
    ready_rx: Option<mpsc::Receiver<()>>,
}

#[cfg(feature = "cpal-backend")]
struct AdaptiveMonitor {
    device: cpal::Device,
    sample_format: cpal::SampleFormat,
    stream_cfg: cpal::StreamConfig,
    config: AudioPostProcessConfig,
    runtime: SharedAudioDataPlane,
    synth_sample_rate: u32,
}

#[cfg(feature = "cpal-backend")]
struct MonitorState {
    last_underruns: u64,
    consecutive_underruns: u64,
    clean_seconds: u64,
    current_size: Option<u32>,
    last_pcm_len: usize,
    last_dropped_stale: u64,
    was_below_target: bool,
    was_above_soft_cap: bool,
}

#[cfg(feature = "cpal-backend")]
impl AdaptiveMonitor {
    const UNDERRUNS_PER_SEC_THRESHOLD: u64 = 10;
    const UNDERRUN_SECS_THRESHOLD: u64 = 3;
    const RECOVERY_SECS_THRESHOLD: u64 = 60;
    const MIN_BUFFER_MIN: u32 = 128;
    const MIN_RECOVERY_BUFFER: u32 = 1024;

    fn monitor_metrics(&mut self, state: &mut MonitorState) {
        let pcm_len = self.runtime.pcm_len();
        let target_latency = self.runtime.target_latency_samples();
        let soft_cap = self.runtime.pcm_soft_cap_samples();
        let below_target = target_latency > 0 && pcm_len < target_latency;
        let above_soft_cap = soft_cap > 0 && pcm_len > soft_cap;
        let delta_pcm = if pcm_len >= state.last_pcm_len {
            pcm_len - state.last_pcm_len
        } else {
            state.last_pcm_len - pcm_len
        };
        let change_threshold = target_latency.max(256);
        if delta_pcm >= change_threshold {
            log::info!(
                "CPAL buffer status: pcm_len={} (delta={}), target_latency={}, soft_cap={}, buffer_size={:?}",
                pcm_len,
                delta_pcm,
                target_latency,
                soft_cap,
                self.stream_cfg.buffer_size
            );
            state.last_pcm_len = pcm_len;
        }
        if below_target && !state.was_below_target {
            log::info!(
                "CPAL buffer status: pcm_len dropped below target_latency (pcm_len={}, target_latency={}, buffer_size={:?})",
                pcm_len,
                target_latency,
                self.stream_cfg.buffer_size
            );
        }
        if above_soft_cap && !state.was_above_soft_cap {
            log::warn!(
                "CPAL buffer status: pcm_len exceeded soft_cap (pcm_len={}, soft_cap={}, buffer_size={:?})",
                pcm_len,
                soft_cap,
                self.stream_cfg.buffer_size
            );
        }
        state.was_below_target = below_target;
        state.was_above_soft_cap = above_soft_cap;

        let current = self.runtime.underrun_samples();
        let delta = current.saturating_sub(state.last_underruns);
        state.last_underruns = current;

        if delta >= Self::UNDERRUNS_PER_SEC_THRESHOLD {
            log::warn!(
                "CPAL underrun spike: +{} in 1s (pcm_len={}, target_latency={}, buffer_size={:?})",
                delta,
                pcm_len,
                target_latency,
                self.stream_cfg.buffer_size
            );
        }

        let dropped_stale = self.runtime.dropped_stale_events();
        if dropped_stale > state.last_dropped_stale {
            log::warn!(
                "CPAL engine dropped stale writes: +{} (total={})",
                dropped_stale - state.last_dropped_stale,
                dropped_stale
            );
            state.last_dropped_stale = dropped_stale;
        }

        if delta > Self::UNDERRUNS_PER_SEC_THRESHOLD {
            state.consecutive_underruns += 1;
            state.clean_seconds = 0;
        } else if delta == 0 {
            state.consecutive_underruns = 0;
            state.clean_seconds += 1;
        } else {
            state.consecutive_underruns = 0;
            state.clean_seconds = 0;
        }
    }

    fn rebalance_stream(
        &mut self,
        stream: &mut Option<cpal::Stream>,
        state: &mut MonitorState,
    ) {
        use cpal::traits::StreamTrait;

        if state.consecutive_underruns >= Self::UNDERRUN_SECS_THRESHOLD {
            let base = state.current_size.unwrap_or(Self::MIN_BUFFER_MIN);
            let desired_min = base.saturating_mul(2);
            if let Some(buffer_size) = CpalHost::pick_buffer_size(
                &self.device,
                self.sample_format,
                self.stream_cfg.channels,
                self.stream_cfg.sample_rate.0,
                desired_min,
            ) {
                let new_size = match buffer_size {
                    cpal::BufferSize::Fixed(v) => v,
                    _ => base,
                };
                if state.current_size != Some(new_size) {
                    self.stream_cfg.buffer_size = buffer_size;
                    let new_stream = CpalHost::build_stream(
                        &self.device,
                        self.sample_format,
                        &self.stream_cfg,
                        &self.config,
                        self.runtime.clone(),
                        self.synth_sample_rate,
                        self.config.channels.max(1),
                    );
                    if let Some(new_stream) = new_stream {
                        if new_stream.play().is_ok() {
                            *stream = Some(new_stream);
                            if let cpal::BufferSize::Fixed(size) = self.stream_cfg.buffer_size {
                                state.current_size = Some(size);
                                log::warn!("CPAL: increased buffer_size to {}", size);
                                CpalHost::update_soft_cap(&self.runtime);
                            }
                            CpalHost::log_output_config(
                                &self.stream_cfg,
                                self.sample_format,
                                &self.runtime,
                                self.synth_sample_rate,
                            );
                        }
                    }
                }
            }
            state.consecutive_underruns = 0;
        }

        if let Some(size) = state.current_size
            && size > Self::MIN_RECOVERY_BUFFER
            && state.clean_seconds >= Self::RECOVERY_SECS_THRESHOLD
        {
            let desired = (size / 2).max(Self::MIN_RECOVERY_BUFFER);
            let desired_min = desired;
            if let Some(buffer_size) = CpalHost::pick_buffer_size(
                &self.device,
                self.sample_format,
                self.stream_cfg.channels,
                self.stream_cfg.sample_rate.0,
                desired_min,
            ) {
                if let cpal::BufferSize::Fixed(new_size) = buffer_size {
                    if new_size < size {
                        self.stream_cfg.buffer_size = buffer_size;
                        let new_stream = CpalHost::build_stream(
                            &self.device,
                            self.sample_format,
                            &self.stream_cfg,
                            &self.config,
                            self.runtime.clone(),
                            self.synth_sample_rate,
                            self.config.channels.max(1),
                        );
                        if let Some(new_stream) = new_stream {
                            if new_stream.play().is_ok() {
                                *stream = Some(new_stream);
                                state.current_size = Some(new_size);
                                log::info!("CPAL: reduced buffer_size to {}", new_size);
                                state.clean_seconds = 0;
                                CpalHost::update_soft_cap(&self.runtime);
                                CpalHost::log_output_config(
                                    &self.stream_cfg,
                                    self.sample_format,
                                    &self.runtime,
                                    self.synth_sample_rate,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn run(mut self, ready_tx: mpsc::Sender<()>) {
        use cpal::traits::StreamTrait;

        let mut stream = CpalHost::build_stream(
            &self.device,
            self.sample_format,
            &self.stream_cfg,
            &self.config,
            self.runtime.clone(),
            self.synth_sample_rate,
            self.config.channels.max(1),
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
            );
        }

        let stream = match stream {
            Some(stream) => stream,
            None => {
                log::warn!("Unable to build CPAL output stream");
                return;
            }
        };
        let _ = ready_tx.send(());
        // Stream start delay disabled while priming gate handles startup.
        if let Err(err) = stream.play() {
            log::warn!("Unable to start CPAL output stream: {err}");
            return;
        }
        CpalHost::update_soft_cap(&self.runtime);
        CpalHost::log_output_config(
            &self.stream_cfg,
            self.sample_format,
            &self.runtime,
            self.synth_sample_rate,
        );
        let mut stream = Some(stream);

        let mut state = MonitorState {
            last_underruns: self.runtime.underrun_samples(),
            consecutive_underruns: 0,
            clean_seconds: 0,
            current_size: match self.stream_cfg.buffer_size {
                cpal::BufferSize::Fixed(size) => Some(size),
                _ => None,
            },
            last_pcm_len: self.runtime.pcm_len(),
            last_dropped_stale: self.runtime.dropped_stale_events(),
            was_below_target: false,
            was_above_soft_cap: false,
        };

        loop {
            thread::sleep(Duration::from_secs(1));
            if stream.is_none() {
                return;
            }
            self.monitor_metrics(&mut state);
            self.rebalance_stream(&mut stream, &mut state);
        }
    }
}

impl CpalHost {
    #[cfg(feature = "cpal-backend")]
    fn update_soft_cap(runtime: &SharedAudioDataPlane) {
        let target_latency_samples = runtime.target_latency_samples();
        let staging_capacity = runtime.pcm_capacity_samples();
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
        runtime.set_pcm_soft_cap_samples(soft_cap);
        log::info!(
            "Audio buffer policy: soft_cap={} samples (target_latency={}, staging_capacity={})",
            soft_cap,
            target_latency_samples,
            staging_capacity
        );
    }

    /// Creates a host and starts the output stream when CPAL is enabled.
    pub fn new(post_processor: AudioPostProcessor, config: AudioPostProcessConfig) -> Self {
        #[cfg(feature = "cpal-backend")]
        let (ready_tx, ready_rx) = mpsc::channel();
        #[cfg(feature = "cpal-backend")]
        let monitor = Self::start_stream(post_processor, config, ready_tx);

        #[cfg(not(feature = "cpal-backend"))]
        {
            let _ = config;
            log::info!("CPAL backend disabled; build with feature 'cpal-backend' to enable audio output stream");
            return Self {
                _post_processor: post_processor,
            };
        }

        #[cfg(feature = "cpal-backend")]
        {
            return Self {
                _monitor: monitor,
                ready_rx: Some(ready_rx),
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
    fn start_stream(
        post_processor: AudioPostProcessor,
        mut config: AudioPostProcessConfig,
        ready_tx: mpsc::Sender<()>,
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
        let preferred_min = 128u32;
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
            log::info!("CPAL: using default buffer_size={:?}", stream_cfg.buffer_size);
        }
        config.sample_rate_hz = stream_cfg.sample_rate.0;
        config.channels = stream_cfg.channels as usize;

        let runtime = post_processor.shared_data_plane();
        let synth_sample_rate = post_processor.synth_sample_rate();
        Self::update_soft_cap(&runtime);
        let monitor = Self::spawn_adaptive_monitor(
            device,
            sample_format,
            stream_cfg,
            config,
            runtime,
            synth_sample_rate,
            ready_tx,
        );

        Some(monitor)
    }

    #[cfg(feature = "cpal-backend")]
    fn build_stream(
        device: &cpal::Device,
        sample_format: cpal::SampleFormat,
        stream_cfg: &cpal::StreamConfig,
        config: &AudioPostProcessConfig,
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        channels: usize,
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
                let mut post_processor = AudioPostProcessor::new(runtime, synth_sample_rate);
                let config = config.clone();
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [f32], _| {
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
                let mut post_processor = AudioPostProcessor::new(runtime, synth_sample_rate);
                let config = config.clone();
                let mut scratch: Vec<f32> = Vec::new();
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [i16], _| {
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
                let mut post_processor = AudioPostProcessor::new(runtime, synth_sample_rate);
                let config = config.clone();
                let mut scratch: Vec<f32> = Vec::new();
                match device.build_output_stream(
                    stream_cfg,
                    move |data: &mut [u16], _| {
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
    fn spawn_adaptive_monitor(
        device: cpal::Device,
        sample_format: cpal::SampleFormat,
        stream_cfg: cpal::StreamConfig,
        config: AudioPostProcessConfig,
        runtime: SharedAudioDataPlane,
        synth_sample_rate: u32,
        ready_tx: mpsc::Sender<()>,
    ) -> std::thread::JoinHandle<()> {
        let monitor = AdaptiveMonitor {
            device,
            sample_format,
            stream_cfg,
            config,
            runtime,
            synth_sample_rate,
        };

        thread::spawn(move || monitor.run(ready_tx))
    }

    #[cfg(feature = "cpal-backend")]
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
