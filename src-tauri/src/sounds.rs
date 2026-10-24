//! Tiny synthesized UI sounds (no asset files, no extra deps).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum Cue {
    Start,
    Stop,
    Error,
}

pub fn play(cue: Cue) {
    std::thread::spawn(move || {
        if let Err(e) = play_blocking(cue) {
            log::debug!("sound cue failed: {e:#}");
        }
    });
}

fn play_blocking(cue: Cue) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.config();
    let rate = config.sample_rate as f32;
    let channels = config.channels as usize;

    // (frequency, duration ms) notes
    let notes: Vec<(f32, u32)> = match cue {
        Cue::Start => vec![(660.0, 55), (880.0, 70)],
        Cue::Stop => vec![(880.0, 55), (587.0, 80)],
        Cue::Error => vec![(330.0, 90), (262.0, 120)],
    };
    let mut samples: Vec<f32> = Vec::new();
    for (freq, ms) in notes {
        let n = (rate * ms as f32 / 1000.0) as usize;
        for i in 0..n {
            let t = i as f32 / rate;
            let env = {
                let a = (i as f32 / (0.004 * rate)).min(1.0);
                let r = ((n - i) as f32 / (0.02 * rate)).min(1.0);
                a * r
            };
            samples.push((2.0 * std::f32::consts::PI * freq * t).sin() * 0.18 * env);
        }
    }
    let total = samples.len();
    let samples = Arc::new(samples);
    let pos = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);

    let s2 = samples.clone();
    let p2 = pos.clone();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config.clone(),
            move |out: &mut [f32], _| {
                let mut i = p2.load(Ordering::Relaxed);
                for frame in out.chunks_mut(channels) {
                    let v = if i < total { s2[i] } else { 0.0 };
                    for ch in frame.iter_mut() {
                        *ch = v;
                    }
                    i += 1;
                }
                p2.store(i, Ordering::Relaxed);
                if i >= total + 2048 {
                    let _ = done_tx.try_send(());
                }
            },
            |e| log::debug!("sound stream error: {e}"),
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            config.clone(),
            move |out: &mut [i16], _| {
                let mut i = p2.load(Ordering::Relaxed);
                for frame in out.chunks_mut(channels) {
                    let v = if i < total { (s2[i] * i16::MAX as f32) as i16 } else { 0 };
                    for ch in frame.iter_mut() {
                        *ch = v;
                    }
                    i += 1;
                }
                p2.store(i, Ordering::Relaxed);
                if i >= total + 2048 {
                    let _ = done_tx.try_send(());
                }
            },
            |e| log::debug!("sound stream error: {e}"),
            None,
        )?,
        other => anyhow::bail!("unsupported output format {other:?}"),
    };
    stream.play()?;
    let _ = done_rx.recv_timeout(std::time::Duration::from_millis(600));
    drop(stream);
    Ok(())
}
