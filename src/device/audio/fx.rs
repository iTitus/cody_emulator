//! Audio effects used by the postprocessor.

/// Clone support for boxed audio effects.
pub trait AudioEffectClone {
    fn clone_box(&self) -> Box<dyn AudioEffect>;
}

impl<T> AudioEffectClone for T
where
    T: 'static + AudioEffect + Clone,
{
    fn clone_box(&self) -> Box<dyn AudioEffect> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn AudioEffect> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Stateful audio effect component that mutates audio buffers in-place.
pub trait AudioEffect: Send + AudioEffectClone {
    fn prepare(&mut self, channels: usize, sample_rate_hz: u32);
    fn process_in_place(&mut self, buffer: &mut [f32]);
    fn reset(&mut self);
}

/// Multiplies each sample by a constant gain factor.
#[derive(Debug, Clone, Copy)]
pub struct GainEffect {
    gain: f32,
}

impl GainEffect {
    pub const fn new(gain: f32) -> Self {
        Self { gain }
    }
}

impl AudioEffect for GainEffect {
    fn prepare(&mut self, _channels: usize, _sample_rate_hz: u32) {}

    fn process_in_place(&mut self, buffer: &mut [f32]) {
        for sample in buffer.iter_mut() {
            *sample *= self.gain;
        }
    }

    fn reset(&mut self) {}
}

/// Applies a cheap saturating soft clip transfer function.
#[derive(Debug, Default, Clone, Copy)]
pub struct SoftClipEffect;

impl AudioEffect for SoftClipEffect {
    fn prepare(&mut self, _channels: usize, _sample_rate_hz: u32) {}

    fn process_in_place(&mut self, buffer: &mut [f32]) {
        for sample in buffer.iter_mut() {
            *sample = *sample / (1.0 + sample.abs());
        }
    }

    fn reset(&mut self) {}
}

#[derive(Debug, Clone, Copy, Default)]
struct DcBlockState {
    x1: f32,
    y1: f32,
}

/// First-order DC blocker using `alpha` as feedback coefficient.
#[derive(Debug, Clone)]
pub struct DcBlockEffect {
    alpha: f32,
    state: Vec<DcBlockState>,
}

impl DcBlockEffect {
    pub const fn new(alpha: f32) -> Self {
        Self {
            alpha,
            state: Vec::new(),
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        let channels = channels.max(1);
        if self.state.len() < channels {
            self.state.resize(channels, DcBlockState::default());
        }
    }
}

impl AudioEffect for DcBlockEffect {
    fn prepare(&mut self, channels: usize, _sample_rate_hz: u32) {
        self.ensure_channels(channels);
    }

    fn process_in_place(&mut self, buffer: &mut [f32]) {
        let channels = self.state.len().max(1);
        let alpha = self.alpha.clamp(0.0, 0.9999);
        for (i, sample) in buffer.iter_mut().enumerate() {
            let state = &mut self.state[i % channels];
            let y = *sample - state.x1 + alpha * state.y1;
            state.x1 = *sample;
            state.y1 = y;
            *sample = y;
        }
    }

    fn reset(&mut self) {
        for state in &mut self.state {
            *state = DcBlockState::default();
        }
    }
}

/// First-order low-pass filter with cutoff in hertz.
#[derive(Debug, Clone)]
pub struct OnePoleLowPassEffect {
    cutoff_hz: f32,
    state: Vec<f32>,
    alpha: f32,
}

/// First-order high-pass filter with cutoff in hertz.
#[derive(Debug, Clone)]
pub struct OnePoleHighPassEffect {
    cutoff_hz: f32,
    state: Vec<DcBlockState>,
    alpha: f32,
}

impl OnePoleHighPassEffect {
    pub const fn new(cutoff_hz: f32) -> Self {
        Self {
            cutoff_hz,
            state: Vec::new(),
            alpha: 0.0,
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        let channels = channels.max(1);
        if self.state.len() < channels {
            self.state.resize(channels, DcBlockState::default());
        }
    }
}

impl OnePoleLowPassEffect {
    pub const fn new(cutoff_hz: f32) -> Self {
        Self {
            cutoff_hz,
            state: Vec::new(),
            alpha: 0.0,
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        let channels = channels.max(1);
        if self.state.len() < channels {
            self.state.resize(channels, 0.0);
        }
    }
}

impl AudioEffect for OnePoleLowPassEffect {
    fn prepare(&mut self, channels: usize, sample_rate_hz: u32) {
        self.ensure_channels(channels);
        let sample_rate_hz = sample_rate_hz.max(1);
        let cutoff_hz = self
            .cutoff_hz
            .clamp(1.0, (sample_rate_hz / 2).max(1) as f32);
        let dt = 1.0 / sample_rate_hz as f32;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        self.alpha = dt / (rc + dt);
    }

    fn process_in_place(&mut self, buffer: &mut [f32]) {
        let channels = self.state.len().max(1);
        let alpha = self.alpha;
        for (i, sample) in buffer.iter_mut().enumerate() {
            let state = &mut self.state[i % channels];
            *state += alpha * (*sample - *state);
            *sample = *state;
        }
    }

    fn reset(&mut self) {
        for state in &mut self.state {
            *state = 0.0;
        }
    }
}

impl AudioEffect for OnePoleHighPassEffect {
    fn prepare(&mut self, channels: usize, sample_rate_hz: u32) {
        self.ensure_channels(channels);
        let sample_rate_hz = sample_rate_hz.max(1);
        let cutoff_hz = self
            .cutoff_hz
            .clamp(1.0, (sample_rate_hz / 2).max(1) as f32);
        let dt = 1.0 / sample_rate_hz as f32;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
        self.alpha = rc / (rc + dt);
    }

    fn process_in_place(&mut self, buffer: &mut [f32]) {
        let channels = self.state.len().max(1);
        let alpha = self.alpha;
        for (i, sample) in buffer.iter_mut().enumerate() {
            let state = &mut self.state[i % channels];
            let y = alpha * (state.y1 + *sample - state.x1);
            state.x1 = *sample;
            state.y1 = y;
            *sample = y;
        }
    }

    fn reset(&mut self) {
        for state in &mut self.state {
            *state = DcBlockState::default();
        }
    }
}
