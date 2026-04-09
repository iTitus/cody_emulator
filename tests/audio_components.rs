use cody_emulator::device::audio::engine::{
    AudioControlPlane,
    AudioEngine,
    AudioEvent,
    AudioDataPlane,
};
use cody_emulator::device::audio::AudioConfig;
use cody_emulator::device::audio::post_buffer_policy::BufferState;
use cody_emulator::device::audio::fx::{
    AudioEffect,
    DcBlockEffect,
    GainEffect,
    OnePoleHighPassEffect,
    OnePoleLowPassEffect,
    SoftClipEffect,
};
use cody_emulator::device::audio::postprocess::{
    AudioPostProcessConfig,
    AudioPostProcessor,
};
use cody_emulator::device::audio::mmiodev::{AudioMmioDevice, AUDIO_BASE, AUDIO_REGISTER_COUNT};
use cody_emulator::device::audio::queue::{LockFreePcmRingBuffer, LockFreeQueue};
use cody_emulator::device::audio::registers::AudioRegister;
use cody_emulator::device::audio::synth::SidLikeSynth;
use cody_emulator::cpu;
use cody_emulator::cpu::Cpu;
use cody_emulator::memory::contiguous::Contiguous;
use cody_emulator::memory::mapped::MappedMemory;
use cody_emulator::memory::Memory;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn configure_voice3_saw(synth: &mut SidLikeSynth) {
    synth.write_register(AudioRegister::V2FreqLo.as_u8(), 0xFF);
    synth.write_register(AudioRegister::V2FreqHi.as_u8(), 0xFF);
    synth.write_register(AudioRegister::V2Ad.as_u8(), 0x00);
    synth.write_register(AudioRegister::V2Sr.as_u8(), 0xF0);
    synth.write_register(AudioRegister::V2Control.as_u8(), 0x20 | 0x01);
}

#[test]
fn audio_register_roundtrip_and_invalid_decode() {
    let reg = AudioRegister::from_u8(AudioRegister::V1PwHi.as_u8());
    assert_eq!(reg, Some(AudioRegister::V1PwHi));
    assert_eq!(AudioRegister::from_u8(0x19), None);
}

#[test]
fn lockfree_queue_drops_oldest_when_full() {
    let q = LockFreeQueue::with_capacity(2);
    q.push_drop_oldest(1u8);
    q.push_drop_oldest(2u8);
    q.push_drop_oldest(3u8);

    assert_eq!(q.len(), 2);
    assert_eq!(q.overrun_count(), 1);
    assert_eq!(q.pop_front(), Some(2));
    assert_eq!(q.pop_front(), Some(3));
    assert!(q.is_empty());
}

#[test]
fn lockfree_queue_drain_preserves_order() {
    let q = LockFreeQueue::with_capacity(4);
    q.push_drop_oldest(10u8);
    q.push_drop_oldest(11u8);
    q.push_drop_oldest(12u8);

    let mut drained = Vec::new();
    q.drain_into(&mut drained);
    assert_eq!(drained, vec![10, 11, 12]);
    assert!(q.is_empty());
}

#[test]
fn pcm_ring_buffer_tracks_overrun_and_underrun() {
    let pcm = LockFreePcmRingBuffer::with_capacity(2);
    pcm.push_samples(&[0.1, 0.2, 0.3]);

    assert_eq!(pcm.len(), 2);
    assert_eq!(pcm.overrun_samples(), 1);

    let mut out = Vec::new();
    pcm.pop_samples(3, &mut out);

    assert_eq!(out.len(), 2);
    assert!(approx_eq(out[0], 0.2, 1e-6));
    assert!(approx_eq(out[1], 0.3, 1e-6));
    assert_eq!(pcm.underrun_samples(), 1);
}

#[test]
fn pcm_ring_buffer_pop_front_discards_oldest() {
    let pcm = LockFreePcmRingBuffer::with_capacity(4);
    pcm.push_samples(&[0.25, 0.5]);
    pcm.pop_front();

    let mut out = Vec::new();
    pcm.pop_samples(1, &mut out);
    assert_eq!(out.len(), 1);
    assert!(approx_eq(out[0], 0.5, 1e-6));
}

#[test]
fn synth_rejects_readonly_writes_and_generates_readback() {
    let mut synth = SidLikeSynth::new();

    synth.write_register(AudioRegister::Osc3Read.as_u8(), 0xAA);
    synth.write_register(AudioRegister::Env3Read.as_u8(), 0xBB);
    assert_eq!(synth.read_register(AudioRegister::Osc3Read.as_u8()), 0);
    assert_eq!(synth.read_register(AudioRegister::Env3Read.as_u8()), 0);

    synth.write_register(AudioRegister::V2FreqLo.as_u8(), 0xFF);
    synth.write_register(AudioRegister::V2FreqHi.as_u8(), 0xFF);
    synth.write_register(AudioRegister::V2Ad.as_u8(), 0x00);
    synth.write_register(AudioRegister::V2Sr.as_u8(), 0xF0);
    synth.write_register(
        AudioRegister::V2Control.as_u8(),
        0x20 | 0x01, // saw + gate
    );
    synth.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x0F);

    let sample = synth.render_sample(16_000);
    assert!((-1.0..=1.0).contains(&sample));

    let osc3 = synth.osc3_readback();
    let env3 = synth.env3_readback();
    assert_ne!(osc3, 0);
    assert_ne!(env3, 0);
}

#[test]
fn synth_volume_zero_silences_output() {
    let mut synth = SidLikeSynth::new();
    configure_voice3_saw(&mut synth);

    synth.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x00);
    let muted = synth.render_sample(16_000);
    assert!(approx_eq(muted, 0.0, 1e-9));

    synth.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x0F);
    let mut max_abs = 0.0f32;
    for _ in 0..8 {
        let audible = synth.render_sample(16_000).abs();
        if audible > max_abs {
            max_abs = audible;
        }
    }
    assert!(max_abs > 0.01);
}

#[test]
fn synth_voice3_mute_bit_removes_voice3_from_mix() {
    let mut synth = SidLikeSynth::new();
    configure_voice3_saw(&mut synth);

    synth.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x0F);
    let with_voice3 = synth.render_sample(16_000);

    synth.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x8F);
    let without_voice3 = synth.render_sample(16_000);

    assert!(with_voice3.abs() > 0.0001);
    assert!(approx_eq(without_voice3, 0.0, 1e-9));
}

#[test]
fn engine_resolves_readback_requests_after_forced_catchup() {
    let runtime = Arc::new(AudioDataPlane::new(512));
    let control = Arc::new(AudioControlPlane::new(512));
    let config = AudioConfig::new(1_000_000.0, 16_000, 256.0);
    let mut engine = AudioEngine::new(Arc::clone(&runtime), Arc::clone(&control), config);

    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2FreqLo.as_u8(),
        value: 0xFF,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2FreqHi.as_u8(),
        value: 0xFF,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Ad.as_u8(),
        value: 0x00,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Sr.as_u8(),
        value: 0xF0,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Control.as_u8(),
        value: 0x20 | 0x01, // saw + gate
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::FilterModeVolume.as_u8(),
        value: 0x0F,
    });

    let value = engine.resolve_readback_value(
        100,
        AudioRegister::Osc3Read.as_u8(),
        100,
    );
    assert_ne!(value, 0);
}

#[test]
fn engine_regular_advance_respects_target_latency() {
    let runtime = Arc::new(AudioDataPlane::new(512));
    let control = Arc::new(AudioControlPlane::new(512));
    let config = AudioConfig::new(1_000_000.0, 16_000, 256.0);
    let mut engine = AudioEngine::new(Arc::clone(&runtime), Arc::clone(&control), config);

    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2FreqLo.as_u8(),
        value: 0xFF,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2FreqHi.as_u8(),
        value: 0xFF,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Ad.as_u8(),
        value: 0x00,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Sr.as_u8(),
        value: 0xF0,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::V2Control.as_u8(),
        value: 0x20 | 0x01,
    });
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::FilterModeVolume.as_u8(),
        value: 0x0F,
    });

    engine.advance_to_cpu_cycle(100);
    assert_eq!(runtime.pcm_len(), 0);

    engine.advance_to_cpu_cycle(400);
    let mut produced = Vec::new();
    runtime.pop_pcm_samples(1, &mut produced);
    assert_eq!(produced.len(), 1);
    assert!(produced[0].abs() > 0.0001);
}

#[test]
fn engine_snapshot_updates_only_when_dirty() {
    let runtime = Arc::new(AudioDataPlane::new(512));
    let control = Arc::new(AudioControlPlane::new(512));
    let config = AudioConfig::new(1_000_000.0, 16_000, 256.0);
    let mut engine = AudioEngine::new(Arc::clone(&runtime), Arc::clone(&control), config);

    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 0,
        register: AudioRegister::FilterModeVolume.as_u8(),
        value: 0x0F,
    });

    engine.advance_to_cpu_cycle(400);
    assert_eq!(runtime.snapshot_update_count(), 2);
    assert!(runtime.snapshot_clone().is_some());
    let mut drained = Vec::new();
    runtime.pop_pcm_samples(runtime.pcm_len(), &mut drained);

    // No new writes: snapshot should not refresh.
    engine.advance_to_cpu_cycle(800);
    assert_eq!(runtime.snapshot_update_count(), 2);
    drained.clear();
    runtime.pop_pcm_samples(runtime.pcm_len(), &mut drained);

    // New write batch marks dirty and triggers one more capture.
    control.write_events.push_drop_oldest(AudioEvent {
        cycle: 900,
        register: AudioRegister::V2FreqLo.as_u8(),
        value: 0xAA,
    });

    engine.advance_to_cpu_cycle(1300);
    assert_eq!(runtime.snapshot_update_count(), 3);
    let snap = runtime.snapshot_clone().expect("missing synth snapshot");
    assert_eq!(snap.read_register(AudioRegister::V2FreqLo.as_u8()), 0xAA);
}

#[test]
fn host_pulls_interleaved_and_applies_gain() {
    let runtime = Arc::new(AudioDataPlane::new(512));
    runtime.set_target_latency_samples(1);
    runtime.push_pcm_samples(&[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);

    let mut host = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    host.set_post_effects(vec![Box::new(GainEffect::new(0.5))]);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 2,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 8];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 8);
    for s in out {
        assert!(approx_eq(s, 0.25, 1e-6));
    }
}

#[test]
fn host_zero_frames_returns_empty() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    let mut host = AudioPostProcessor::new(runtime, 16_000);
    let cfg = AudioPostProcessConfig::default();
    let mut out: Vec<f32> = Vec::new();
    host.render_interleaved(&mut out, &cfg);
    assert!(out.is_empty());
}

#[test]
fn host_channels_zero_clamps_to_mono() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    runtime.push_pcm_samples(&[0.2, 0.2, 0.2, 0.2]);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 0,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 3];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 3);
}

#[test]
fn host_underrun_holds_last_sample() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    runtime.push_pcm_samples(&[0.7]);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 3];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 3);
    assert!(approx_eq(out[0], 0.7, 1e-6));
    assert!(approx_eq(out[1], 0.7, 1e-6));
    assert!(approx_eq(out[2], 0.7, 1e-6));
    // Explicitly check buffer state
    assert!(matches!(host.buffer_state(), BufferState::UnderrunHold));
}

#[test]
fn host_underrun_uses_shadow_synth_when_snapshot_available() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    runtime.set_target_latency_samples(1);
    runtime.push_pcm_samples(&[0.0]);
    let mut snap = SidLikeSynth::new();
    configure_voice3_saw(&mut snap);
    snap.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x0F);
    runtime.store_snapshot(snap);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    // No PCM queued: output should come from shadow synth, not silence.
    let mut out = vec![0.0; 6];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 6);
    assert!(out.iter().any(|s| s.abs() > 0.0001));
    // Explicitly check buffer state
    assert!(matches!(host.buffer_state(), BufferState::UnderrunShadow));
}

#[test]
fn host_overrun_fast_forwards_old_pcm() {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    let mut samples = Vec::with_capacity(3000);
    for i in 0..3000 {
        samples.push(i as f32);
    }
    runtime.push_pcm_samples(&samples);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 4];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 4);
    // Overrun should avoid replaying from the stale beginning of the queue.
    assert!(out[0] > 200.0);
    // Explicitly check buffer state
    assert!(matches!(host.buffer_state(), BufferState::Overrun));
}

#[test]
fn host_hard_skip_and_snapshot_fallback() {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    runtime.set_target_latency_samples(1);
    runtime.set_pcm_soft_cap_samples(32);
    let mut samples = Vec::with_capacity(256);
    for i in 0..256 {
        samples.push(i as f32);
    }
    runtime.push_pcm_samples(&samples);

    let mut snap = SidLikeSynth::new();
    configure_voice3_saw(&mut snap);
    snap.write_register(AudioRegister::FilterModeVolume.as_u8(), 0x0F);
    runtime.store_snapshot(snap);

    let mut host = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    let before = runtime.pcm_len();
    let mut out = vec![0.0; 8];
    host.render_interleaved(&mut out, &cfg);
    let after = runtime.pcm_len();

    assert!(before.saturating_sub(after) > 100);

    runtime.push_pcm_samples(&[0.0]);
    let mut out2 = vec![0.0; 6];
    host.render_interleaved(&mut out2, &cfg);
    assert!(out2.iter().any(|s| s.abs() > 0.0001));
}

#[test]
fn host_catchup_skips_when_buffered_over_drift_guard() {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    runtime.set_target_latency_samples(128);
    runtime.set_pcm_soft_cap_samples(4096);
    let mut samples = Vec::with_capacity(1024);
    for i in 0..1024 {
        samples.push(i as f32);
    }
    runtime.push_pcm_samples(&samples);

    let mut host = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    // Catchup decisions are gated for ~1s after startup.
    std::thread::sleep(Duration::from_millis(1100));

    let before = runtime.pcm_len();
    let mut out = vec![0.0; 1];
    host.render_interleaved(&mut out, &cfg);
    let after = runtime.pcm_len();

    assert!(before.saturating_sub(after) > 3);
}

#[test]
fn host_catchup_idle_below_drift_guard() {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    runtime.set_target_latency_samples(128);
    runtime.set_pcm_soft_cap_samples(4096);
    runtime.push_pcm_samples(&[0.1; 200]);

    let mut host = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    std::thread::sleep(Duration::from_millis(200));

    let before = runtime.pcm_len();
    let mut out = vec![0.0; 1];
    host.render_interleaved(&mut out, &cfg);
    let after = runtime.pcm_len();

    assert_eq!(before.saturating_sub(after), 3);
}

#[test]
fn host_catchup_reserves_callback_demand() {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    runtime.set_target_latency_samples(128);
    runtime.set_pcm_soft_cap_samples(1024);
    runtime.push_pcm_samples(&vec![0.0; 579]);

    let mut host = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    // Simulate a delayed first callback so catch-up credit has time to accumulate.
    std::thread::sleep(Duration::from_millis(1100));

    let mut out = vec![0.0; 1];
    host.render_interleaved(&mut out, &cfg);

    assert!(runtime.pcm_len() >= 128);
}

fn build_policy_host(
    target_latency: usize,
    soft_cap: usize,
    buffered: usize,
    synth_sample_rate: u32,
) -> AudioPostProcessor {
    let runtime = Arc::new(AudioDataPlane::new(4096));
    runtime.set_target_latency_samples(target_latency);
    runtime.set_pcm_soft_cap_samples(soft_cap);
    if buffered > 0 {
        runtime.push_pcm_samples(&vec![0.0; buffered]);
    }
    AudioPostProcessor::new(runtime, synth_sample_rate)
}

#[test]
fn policy_overrun_when_buffered_exceeds_soft_cap() {
    let mut host = build_policy_host(128, 200, 201, 60);
    let next_state = host.preview_buffer_state(1);
    assert!(matches!(next_state, BufferState::Overrun));
}

#[test]
fn policy_catchup_when_buffered_over_drift_guard() {
    let mut host = build_policy_host(128, 1024, 140, 60);
    // Catchup decisions are gated for ~1s after startup.
    std::thread::sleep(Duration::from_millis(1100));
    let next_state = host.preview_buffer_state(1);
    assert!(matches!(next_state, BufferState::Catchup));
}

#[test]
fn policy_normal_when_buffered_within_target() {
    let mut host = build_policy_host(128, 1024, 128, 60);
    let next_state = host.preview_buffer_state(1);
    assert!(matches!(next_state, BufferState::Normal));
}

#[test]
fn policy_reenable_initial_catchup_restores_first_trigger_sensitivity() {
    // With synth_sample_rate=60 and wanted_len=1:
    // target=128, drift_guard=129, first-trigger threshold=128.
    // buffered=129 should trigger catchup only when the one-shot sensitivity is active.
    let mut host = build_policy_host(128, 1024, 129, 60);
    host.set_initial_catchup_enabled(false);
    host.set_initial_catchup_enabled(true);

    std::thread::sleep(Duration::from_millis(1100));

    let next_state = host.preview_buffer_state(1);
    assert!(matches!(next_state, BufferState::Catchup));
}

#[test]
fn host_softclip_bounds_output() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    runtime.push_pcm_samples(&[4.0, -4.0, 4.0, -4.0]);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    host.set_post_effects(vec![Box::new(SoftClipEffect::default())]);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 4];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 4);
    for s in out {
        assert!(s.abs() < 1.0);
    }
}

#[test]
fn host_lowpass_smooths_impulse() {
    let runtime = Arc::new(AudioDataPlane::new(64));
    runtime.push_pcm_samples(&[1.0, 0.0, 0.0, 0.0]);

    let mut host = AudioPostProcessor::new(runtime, 16_000);
    host.set_post_effects(vec![Box::new(OnePoleLowPassEffect::new(100.0))]);
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: 16_000,
        channels: 1,
        preferred_output_buffer_frames: 256,
    };

    let mut out = vec![0.0; 4];
    host.render_interleaved(&mut out, &cfg);
    assert_eq!(out.len(), 4);
    assert!(out[0] > out[1]);
    assert!(out[1] >= out[2]);
}

#[test]
fn dc_block_removes_constant_offset() {
    let mut effect = DcBlockEffect::new(0.9);
    effect.prepare(1, 16_000);
    let mut buffer = vec![1.0; 128];
    effect.process_in_place(&mut buffer);
    let tail: f32 = *buffer.last().unwrap_or(&0.0);
    assert!(tail.abs() < 0.01);

    effect.reset();
    effect.prepare(1, 16_000);
    let mut buffer2 = vec![1.0; 2];
    effect.process_in_place(&mut buffer2);
    assert!(buffer2[0] > 0.5);
}

#[test]
fn highpass_attenuates_dc_signal() {
    let mut effect = OnePoleHighPassEffect::new(200.0);
    effect.prepare(1, 16_000);
    let mut buffer = vec![1.0; 256];
    effect.process_in_place(&mut buffer);
    let tail: f32 = *buffer.last().unwrap_or(&0.0);
    assert!(tail.abs() < 0.01);
}

#[test]
fn mmio_readback_read_forces_current_cycle_resolution() {
    let config = AudioConfig::new(1_000_000.0, 16_000, 256.0);
    let mut dev = AudioMmioDevice::with_timing(config);

    dev.write_u8(AudioRegister::V2FreqLo.as_u8() as u16, 0xFF);
    dev.write_u8(AudioRegister::V2FreqHi.as_u8() as u16, 0xFF);
    dev.write_u8(AudioRegister::V2Ad.as_u8() as u16, 0x00);
    dev.write_u8(AudioRegister::V2Sr.as_u8() as u16, 0xF0);
    dev.write_u8(AudioRegister::V2Control.as_u8() as u16, 0x20 | 0x01);
    dev.write_u8(AudioRegister::FilterModeVolume.as_u8() as u16, 0x0F);

    // Keep last_cycle current for synchronous readback catch-up.
    let _ = dev.update(100);

    let osc3 = dev.read_u8(AudioRegister::Osc3Read.as_u8() as u16);
    let env3 = dev.read_u8(AudioRegister::Env3Read.as_u8() as u16);

    assert_ne!(osc3, 0);
    assert_ne!(env3, 0);
}

#[test]
fn mmio_write_and_read_regular_register_roundtrips() {
    let mut dev = AudioMmioDevice::new();
    let addr = AudioRegister::V0FreqLo.as_u8() as u16;

    dev.write_u8(addr, 0x42);
    assert_eq!(dev.read_u8(addr), 0x42);
}

#[test]
fn mmio_write_to_readonly_register_is_ignored() {
    let mut dev = AudioMmioDevice::new();
    dev.write_u8(AudioRegister::Osc3Read.as_u8() as u16, 0xAB);
    assert_eq!(dev.read_u8(AudioRegister::Osc3Read.as_u8() as u16), 0);
}

#[test]
fn mmio_out_of_range_access_is_safe() {
    let mut dev = AudioMmioDevice::new();
    let in_range = AudioRegister::V1FreqLo.as_u8() as u16;
    dev.write_u8(in_range, 0x55);

    let out_of_range = AUDIO_REGISTER_COUNT;
    assert_eq!(dev.read_u8(out_of_range), 0);
    dev.write_u8(out_of_range, 0x99);

    assert_eq!(dev.read_u8(in_range), 0x55);
}

#[test]
fn payload_pcm_capture_codymelody() {
    let data = std::fs::read("_payloads/codymelody.bin").expect("read melody payload");

    let mut ram = Contiguous::new_ram(0xA000);
    ram.force_write_all(0x0300, &data);
    let propeller_ram = Contiguous::new_ram(0x4000);
    let mut rom = Contiguous::new_rom(0x2000);
    rom.force_write_u16(cpu::RESET_VECTOR - 0xE000, 0x0300);

    let mut memory = MappedMemory::new();
    memory.add_memory(0x0000, 0xA000, ram);
    memory.add_memory(0xA000, 0x4000, propeller_ram);
    memory.add_memory(0xE000, 0x2000, rom);

    let audio = AudioMmioDevice::new();
    let runtime = audio.shared_data_plane();
    memory.add_memory(AUDIO_BASE, AUDIO_REGISTER_COUNT, audio);

    let mut cpu = Cpu::new(memory);
    let mut cycles = 0usize;
    let max_cycles = 3_000_000usize;
    let target_samples = 48_000usize;
    let mut samples = Vec::with_capacity(target_samples.min(100_000));
    let mut chunk = Vec::new();
    let mut next_drain = 0usize;

    while cycles < max_cycles && samples.len() < target_samples {
        cycles += cpu.step_instruction() as usize;
        if cycles >= next_drain {
            let available = runtime.pcm_len();
            if available > 0 {
                runtime.pop_pcm_samples(available, &mut chunk);
                samples.extend(chunk.drain(..));
            }
            next_drain = cycles + 20_000; // drain roughly every 20k cycles (~20ms)
        }
    }

    let available = runtime.pcm_len();
    if available > 0 {
        runtime.pop_pcm_samples(available, &mut chunk);
        samples.extend(chunk.drain(..));
    }

    assert!(!samples.is_empty(), "no PCM samples captured");

    let out_path = std::env::temp_dir().join("codymelody_pcm.csv");
    let mut writer = std::io::BufWriter::new(File::create(&out_path).expect("create pcm dump"));
    writeln!(writer, "index,sample").expect("write header");
    for (i, sample) in samples.iter().enumerate() {
        writeln!(writer, "{i},{sample}").expect("write sample");
    }
    eprintln!("PCM dump written to {}", out_path.display());

    let wav_path = std::env::temp_dir().join("codymelody_pcm.wav");
    write_wav_mono_16k(&wav_path, &samples);
    eprintln!("PCM WAV written to {}", wav_path.display());
}

#[test]
fn payload_postprocessed_pcm_capture_codymelody() {
    let data = std::fs::read("_payloads/codymelody.bin").expect("read melody payload");

    let mut ram = Contiguous::new_ram(0xA000);
    ram.force_write_all(0x0300, &data);
    let propeller_ram = Contiguous::new_ram(0x4000);
    let mut rom = Contiguous::new_rom(0x2000);
    rom.force_write_u16(cpu::RESET_VECTOR - 0xE000, 0x0300);

    let mut memory = MappedMemory::new();
    memory.add_memory(0x0000, 0xA000, ram);
    memory.add_memory(0xA000, 0x4000, propeller_ram);
    memory.add_memory(0xE000, 0x2000, rom);

    let audio = AudioMmioDevice::new();
    let runtime = audio.shared_data_plane();
    let mut post = AudioPostProcessor::new(Arc::clone(&runtime), 16_000);
    memory.add_memory(AUDIO_BASE, AUDIO_REGISTER_COUNT, audio);

    let mut cpu = Cpu::new(memory);
    let mut cycles = 0usize;
    let max_cycles = 3_000_000usize;
    let post_rate = 48_000u32;
    let channels = 2usize;
    let cfg = AudioPostProcessConfig {
        sample_rate_hz: post_rate,
        channels,
        preferred_output_buffer_frames: 256,
    };
    let target_frames = 144_000usize;
    let mut output: Vec<f32> = Vec::with_capacity(target_frames * channels);
    let mut scratch: Vec<f32> = Vec::new();
    let drain_cycles = 20_000usize;
    let mut next_drain = 0usize;

    while cycles < max_cycles && output.len() / channels < target_frames {
        cycles += cpu.step_instruction() as usize;
        if cycles >= next_drain {
            let remaining_frames = target_frames.saturating_sub(output.len() / channels);
            if remaining_frames == 0 {
                break;
            }
            let frames_per_drain = ((drain_cycles as f64 / 1_000_000.0) * post_rate as f64)
                .round()
                .max(1.0) as usize;
            let frames = frames_per_drain.min(remaining_frames);
            scratch.resize(frames * channels, 0.0);
            post.render_interleaved(&mut scratch, &cfg);
            output.extend_from_slice(&scratch);
            next_drain = cycles + drain_cycles;
        }
    }

    let remaining_frames = target_frames.saturating_sub(output.len() / channels);
    if remaining_frames > 0 {
        scratch.resize(remaining_frames * channels, 0.0);
        post.render_interleaved(&mut scratch, &cfg);
        output.extend_from_slice(&scratch);
    }

    assert!(!output.is_empty(), "no postprocessed PCM samples captured");

    let out_path = std::env::temp_dir().join("codymelody_post_pcm.csv");
    let mut writer = std::io::BufWriter::new(File::create(&out_path).expect("create pcm dump"));
    writeln!(writer, "index,sample").expect("write header");
    for (i, sample) in output.iter().enumerate() {
        writeln!(writer, "{i},{sample}").expect("write sample");
    }
    eprintln!("Post PCM dump written to {}", out_path.display());

    let wav_path = std::env::temp_dir().join("codymelody_post_pcm.wav");
    write_wav_interleaved(&wav_path, &output, post_rate, channels as u16);
    eprintln!("Post PCM WAV written to {}", wav_path.display());
}

fn write_wav_mono_16k(path: &std::path::Path, samples: &[f32]) {
    let mut file = std::io::BufWriter::new(File::create(path).expect("create wav"));
    let sample_rate = 16_000u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;

    let mut pcm_bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        pcm_bytes.extend_from_slice(&v.to_le_bytes());
    }

    let data_len = pcm_bytes.len() as u32;
    let riff_len = 36 + data_len;

    file.write_all(b"RIFF").expect("write riff");
    file.write_all(&riff_len.to_le_bytes()).expect("write riff len");
    file.write_all(b"WAVE").expect("write wave");

    file.write_all(b"fmt ").expect("write fmt");
    file.write_all(&16u32.to_le_bytes()).expect("write fmt size");
    file.write_all(&1u16.to_le_bytes()).expect("write pcm tag");
    file.write_all(&channels.to_le_bytes()).expect("write channels");
    file.write_all(&sample_rate.to_le_bytes()).expect("write sample rate");
    file.write_all(&byte_rate.to_le_bytes()).expect("write byte rate");
    file.write_all(&block_align.to_le_bytes()).expect("write block align");
    file.write_all(&bits_per_sample.to_le_bytes()).expect("write bits");

    file.write_all(b"data").expect("write data tag");
    file.write_all(&data_len.to_le_bytes()).expect("write data len");
    file.write_all(&pcm_bytes).expect("write data");
}

fn write_wav_interleaved(
    path: &std::path::Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) {
    let mut file = std::io::BufWriter::new(File::create(path).expect("create wav"));
    let bits_per_sample = 16u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;

    let mut pcm_bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        pcm_bytes.extend_from_slice(&v.to_le_bytes());
    }

    let data_len = pcm_bytes.len() as u32;
    let riff_len = 36 + data_len;

    file.write_all(b"RIFF").expect("write riff");
    file.write_all(&riff_len.to_le_bytes()).expect("write riff len");
    file.write_all(b"WAVE").expect("write wave");

    file.write_all(b"fmt ").expect("write fmt");
    file.write_all(&16u32.to_le_bytes()).expect("write fmt size");
    file.write_all(&1u16.to_le_bytes()).expect("write pcm tag");
    file.write_all(&channels.to_le_bytes()).expect("write channels");
    file.write_all(&sample_rate.to_le_bytes()).expect("write sample rate");
    file.write_all(&byte_rate.to_le_bytes()).expect("write byte rate");
    file.write_all(&block_align.to_le_bytes()).expect("write block align");
    file.write_all(&bits_per_sample.to_le_bytes()).expect("write bits");

    file.write_all(b"data").expect("write data tag");
    file.write_all(&data_len.to_le_bytes()).expect("write data len");
    file.write_all(&pcm_bytes).expect("write data");
}

#[test]
fn mmio_update_returns_no_interrupt() {
    let mut dev = AudioMmioDevice::new();
    let interrupt = dev.update(1234);
    assert!(!interrupt.is_irq());
    assert!(!interrupt.is_nmi());
}
