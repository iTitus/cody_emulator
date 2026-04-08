//! Simple linear resampler used by the audio postprocessor.

use std::collections::VecDeque;

/// Linear resampler for mono source samples.
#[derive(Debug, Clone, Copy)]
pub struct LinearResampler {
    source_pos: f64,
    last_sample: f32,
}

impl LinearResampler {
    /// Creates a linear resampler with reset position and last sample.
    pub const fn new() -> Self {
        Self {
            source_pos: 0.0,
            last_sample: 0.0,
        }
    }

    /// Returns the last interpolated sample value.
    pub const fn last_sample(&self) -> f32 {
        self.last_sample
    }

    /// Overrides the last sample value used for underrun padding.
    pub fn set_last_sample(&mut self, sample: f32) {
        self.last_sample = sample;
    }

    /// Calculates how many source samples are needed for the next render.
    pub fn required_source_len(
        &self,
        frames: usize,
        synth_sample_rate: u32,
        output_sample_rate: u32,
    ) -> usize {
        let ratio = synth_sample_rate as f64 / output_sample_rate.max(1) as f64;
        (self.source_pos + frames as f64 * ratio).ceil() as usize + 2
    }

    /// Renders interleaved output frames using linear interpolation.
    pub fn render_interleaved(
        &mut self,
        source_fifo: &mut VecDeque<f32>,
        out: &mut [f32],
        channels: usize,
        synth_sample_rate: u32,
        output_sample_rate: u32,
    ) -> usize {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        if frames == 0 {
            return 0;
        }

        let ratio = synth_sample_rate as f64 / output_sample_rate.max(1) as f64;
        let mut out_idx = 0usize;
        for _ in 0..frames {
            if source_fifo.len() < 2 {
                source_fifo.push_back(self.last_sample);
                source_fifo.push_back(self.last_sample);
            }

            let idx0 = self.source_pos.floor() as usize;
            let frac = (self.source_pos - idx0 as f64) as f32;
            let s0 = source_fifo.get(idx0).copied().unwrap_or(0.0);
            let s1 = source_fifo.get(idx0 + 1).copied().unwrap_or(s0);
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
                    let _ = source_fifo.pop_front();
                }
                self.source_pos -= consumed as f64;
            }
        }

        out_idx
    }
}
