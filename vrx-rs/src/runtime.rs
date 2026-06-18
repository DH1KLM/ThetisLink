// SPDX-License-Identifier: GPL-2.0-or-later

//! `VrxRuntime` glues the channelizer + Opus encoder + control-state
//! into a single sync `feed()` entry-point. Callers drive it with an
//! IQ batch + current VFO/DDC frequencies; the runtime reads the
//! shared control-state (enable / freq / mode), runs the DSP when
//! enabled, encodes Opus 20 ms frames and invokes a user-supplied
//! callback for each frame.
//!
//! The runtime is intentionally sync and free of async runtimes —
//! the host application's async loop (tokio / async-std / smol /
//! single-threaded) calls `feed()` whenever a new IQ batch arrives.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use log::{info, warn};

use crate::channelizer::{fft_n_for_rates, VrxChannelizer, NB_OUTPUT_RATE_HZ, WB_OUTPUT_RATE_HZ};
use crate::config::AUDIO_BIN_HZ;

/// Convert signed-Hz filter edges (UI convention, like main spectrum)
/// to non-negative audio-bin offsets the channelizer wants. For USB
/// both Hz values are typically positive; for LSB both negative. The
/// runtime takes |Hz|/bin_width and orders lo<hi.
fn filter_to_audio_bins(low_hz: i32, high_hz: i32, mode: crate::config::VrxMode) -> (usize, usize) {
    let (near_hz, far_hz) = match mode {
        crate::config::VrxMode::Usb => (low_hz.max(0), high_hz.max(0)),
        crate::config::VrxMode::Lsb => ((-high_hz).max(0), (-low_hz).max(0)),
        // AM-family: symmetric around carrier. Half-bandwidth = max(|lo|, |hi|).
        crate::config::VrxMode::Am
        | crate::config::VrxMode::Sam
        | crate::config::VrxMode::Fm => (0, low_hz.abs().max(high_hz.abs())),
    };
    let lo = (near_hz as f32 / AUDIO_BIN_HZ).round().max(0.0) as usize;
    let hi = (far_hz as f32 / AUDIO_BIN_HZ).round().max(0.0) as usize;
    (lo.min(hi), lo.max(hi))
}
use crate::config::{VrxConfig, VrxControlState, VrxMode};
use crate::opus::VrxOpusEncoder;
use crate::wav::RollingWavWriter;

pub struct VrxRuntimeOptions {
    /// Identifies this VRX channel (0 = first VRX, 1 = second, ...).
    /// Passed through to the callback so a multi-VRX host can route
    /// frames to the right consumer.
    pub vrx_id: u8,
    /// Optional directory for rolling WAV captures (dev tooling).
    pub wav_dir: Option<String>,
    /// Segment length (s) for rolling WAV. Ignored if `wav_dir` is None.
    pub wav_segment_sec: u32,
    /// Wideband (16 kHz Opus, 256-pt iFFT, audio bandwidth up to 8 kHz)
    /// versus narrowband (8 kHz Opus, 128-pt iFFT, audio up to 4 kHz).
    /// Default narrowband. Affects channelizer iFFT size + Opus rate.
    pub wideband: bool,
}

impl Default for VrxRuntimeOptions {
    fn default() -> Self {
        Self {
            vrx_id: 0,
            wav_dir: None,
            wav_segment_sec: 10,
            wideband: false,
        }
    }
}

/// Callback invoked once per encoded Opus frame (20 ms at 8 kHz NB,
/// `FRAME_SAMPLES` audio samples). The host implements this to ship
/// the audio anywhere it wants — UDP, local playback, file, etc.
pub trait VrxAudioCallback {
    fn on_frame(
        &mut self,
        vrx_id: u8,
        audio_8k: &[f32],
        opus_bytes: &[u8],
        sequence: u32,
    );
}

pub struct VrxRuntime {
    opts_vrx_id: u8,
    output_rate_hz: u32,
    control: Arc<Mutex<VrxControlState>>,
    channelizer: Option<VrxChannelizer>,
    current_mode: VrxMode,
    opus: Option<VrxOpusEncoder>,
    opus_input_buf: Vec<i16>,
    sequence: u32,
    wav: Option<RollingWavWriter>,
    last_logged_offset_hz: i32,
}

impl VrxRuntime {
    pub fn new(opts: VrxRuntimeOptions, control: Arc<Mutex<VrxControlState>>) -> Self {
        let output_rate_hz = if opts.wideband { WB_OUTPUT_RATE_HZ } else { NB_OUTPUT_RATE_HZ };
        let wav = opts.wav_dir.as_ref().map(|dir| {
            info!(
                "VRX{} runtime: WAV-dir={} segment={} s output_rate={} Hz",
                opts.vrx_id + 1,
                dir,
                opts.wav_segment_sec,
                output_rate_hz,
            );
            RollingWavWriter::new(
                PathBuf::from(dir),
                output_rate_hz,
                opts.wav_segment_sec,
            )
        });
        Self {
            opts_vrx_id: opts.vrx_id,
            output_rate_hz,
            control,
            channelizer: None,
            current_mode: VrxMode::Usb,
            opus: None,
            opus_input_buf: Vec::with_capacity(640),
            sequence: 0,
            wav,
            last_logged_offset_hz: i32::MIN,
        }
    }

    /// Output sample rate (8000 for NB, 16000 for WB). Const after construction.
    pub fn output_rate_hz(&self) -> u32 {
        self.output_rate_hz
    }

    /// Feed one batch of IQ samples + current radio state. Reads
    /// `control` for enable/freq/mode each call, runs the channelizer
    /// when enabled, accumulates audio, encodes 20 ms Opus frames and
    /// invokes the callback for each frame. No-op when disabled.
    pub fn feed(
        &mut self,
        iq_sample_rate_hz: u32,
        iq: &[(f32, f32)],
        vfo_hz: u64,
        ddc_center_hz: u64,
        callback: &mut impl VrxAudioCallback,
    ) {
        let (enabled, target_freq_hz, mode, filter_low_hz, filter_high_hz) = {
            let s = self.control.lock().expect("VrxControlState mutex poisoned").clone();
            (s.enabled, s.target_freq_hz, s.mode, s.filter_low_hz, s.filter_high_hz)
        };
        if !enabled {
            // Reset opus accumulator so a stop/start doesn't bleed
            // half-filled frames across the gap.
            self.opus_input_buf.clear();
            return;
        }
        if iq_sample_rate_hz == 0 {
            return;
        }

        // (Re)create channelizer on first call, mode change, OR input
        // rate change. FFT size depends on input rate so a rate flip
        // requires a fresh channelizer with matching bin-width.
        let rate_changed = self
            .channelizer
            .as_ref()
            .map_or(true, |c| c.input_rate_hz() != iq_sample_rate_hz);
        // Translate signed Hz filter to audio-bin offsets per mode
        // (channelizer always wants non-negative distances from carrier).
        let (lo_bins, hi_bins) = filter_to_audio_bins(filter_low_hz, filter_high_hz, mode);

        if self.channelizer.is_none() || self.current_mode != mode || rate_changed {
            let config = VrxConfig {
                carrier_offset_hz: 0.0,
                mode,
                filter_lo_bins: lo_bins,
                filter_hi_bins: hi_bins,
            };
            let fft_n = fft_n_for_rates(iq_sample_rate_hz, self.output_rate_hz);
            self.channelizer = Some(VrxChannelizer::new_with_output_rate(
                config,
                iq_sample_rate_hz,
                self.output_rate_hz,
            ));
            self.current_mode = mode;
            let ifft_n = self.channelizer.as_ref().map(|c| c.ifft_size()).unwrap_or(0);
            let bin_hz = iq_sample_rate_hz as f32 / fft_n.max(1) as f32;
            info!(
                "VRX{} runtime: channelizer (re)built — input_rate={} Hz, FFT_N={}, iFFT={}, output_rate={} Hz, bin={:.1} Hz, mode={:?}",
                self.opts_vrx_id + 1,
                iq_sample_rate_hz,
                fft_n,
                ifft_n,
                self.output_rate_hz,
                bin_hz,
                mode,
            );
        } else if let Some(ch) = self.channelizer.as_mut() {
            // Live filter update: cheap, just mutates two usize fields.
            ch.set_filter(lo_bins, hi_bins);
        }

        // target_freq_hz=0 → fall back to VFO so the listener hears
        // the current passband until the user explicitly tunes.
        let absolute_listen_hz = if target_freq_hz == 0 { vfo_hz } else { target_freq_hz };
        let effective_offset_hz =
            (absolute_listen_hz as i128 - ddc_center_hz as i128) as f32;
        if let Some(ch) = self.channelizer.as_mut() {
            ch.set_carrier_offset(effective_offset_hz);
        }
        // Log effective offset only when it changes by ≥ 1 audio bin
        // width (62.5 Hz). Keeps steady-state quiet, captures tuning.
        {
            let curr = effective_offset_hz as i32;
            if (curr - self.last_logged_offset_hz).abs() >= 63 {
                self.last_logged_offset_hz = curr;
                info!(
                    "VRX{} listen freq: {:.3} kHz (DDC={:.3} kHz, effective_offset={} Hz)",
                    self.opts_vrx_id + 1,
                    absolute_listen_hz as f64 / 1000.0,
                    ddc_center_hz as f64 / 1000.0,
                    curr,
                );
            }
        }

        let audio = self
            .channelizer
            .as_mut()
            .map(|ch| ch.push_iq(iq))
            .unwrap_or_default();
        if audio.is_empty() {
            return;
        }
        if let Some(wav) = self.wav.as_mut() {
            if let Err(e) = wav.push(&audio) {
                warn!("VRX{} runtime: WAV write failed: {}", self.opts_vrx_id + 1, e);
            }
        }

        // Lazily create Opus encoder on first audio (so a never-
        // enabled runtime never instantiates one).
        if self.opus.is_none() {
            match VrxOpusEncoder::new_with_rate(self.output_rate_hz) {
                Ok(e) => self.opus = Some(e),
                Err(e) => {
                    warn!(
                        "VRX{} runtime: Opus encoder init failed: {} — audio frames dropped",
                        self.opts_vrx_id + 1,
                        e
                    );
                    return;
                }
            }
        }

        // f32 → i16 with clipping
        for &s in &audio {
            let clipped = s.clamp(-1.0, 1.0);
            self.opus_input_buf.push((clipped * 32767.0) as i16);
        }

        let encoder = self.opus.as_mut().expect("opus encoder just initialised");
        let frame_samples = encoder.frame_samples();
        while self.opus_input_buf.len() >= frame_samples {
            let frame: Vec<i16> = self.opus_input_buf.drain(..frame_samples).collect();
            let frame_audio_f32: Vec<f32> = frame.iter().map(|&s| s as f32 / 32767.0).collect();
            match encoder.encode(&frame) {
                Ok(opus_bytes) => {
                    callback.on_frame(
                        self.opts_vrx_id,
                        &frame_audio_f32,
                        &opus_bytes,
                        self.sequence,
                    );
                    self.sequence = self.sequence.wrapping_add(1);
                }
                Err(e) => {
                    warn!(
                        "VRX{} runtime: Opus encode failed: {} — frame dropped",
                        self.opts_vrx_id + 1,
                        e
                    );
                }
            }
        }
    }
}
