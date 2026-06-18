// SPDX-License-Identifier: GPL-2.0-or-later

//! FFT-channelizer that converts a wideband complex I/Q stream into
//! narrowband audio at a single tunable carrier offset. Designed for
//! amateur-radio SSB demodulation (USB / LSB) but the bin-selection
//! is generic.
//!
//! The output sample rate is fixed at 8 kHz; `FFT_N` is derived from
//! the input rate at construction so the FFT/iFFT bin-widths match
//! exactly regardless of which DDC rate the host SDR provides
//! (96 / 192 / 384 / 768 / 1536 kHz are all supported).

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

use crate::config::{VrxConfig, VrxMode};

/// Narrowband output (8 kHz, Opus NB): 128-point iFFT → 62.5 Hz/bin,
/// audio bandwidth limited to ~4 kHz. Default.
pub const NB_OUTPUT_RATE_HZ: u32 = 8_000;
pub const NB_IFFT_N: usize = 128;

/// Wideband output (16 kHz, Opus WB): 256-point iFFT → 62.5 Hz/bin
/// (same bin width as NB!), audio bandwidth up to ~8 kHz. For AM/FM
/// quality and avoiding NBFM sideband clipping.
pub const WB_OUTPUT_RATE_HZ: u32 = 16_000;
pub const WB_IFFT_N: usize = 256;

// Legacy aliases kept for non-runtime callers (vrx-spike, examples).
pub const OUTPUT_RATE_HZ: u32 = NB_OUTPUT_RATE_HZ;
pub const IFFT_N: usize = NB_IFFT_N;
pub const OUTPUT_HOP: usize = NB_IFFT_N / 2;

/// Default symmetric SSB bandwidth (3 kHz = 48 audio bins). Kept as a
/// convenience constant; runtime filter is asymmetric (lo + hi).
pub const DEFAULT_BW_BINS: usize = 48;

/// Default FFT size assumed for the legacy 384 kHz DDC rate (kept for
/// the offline synthetic test path in `vrx-spike`).
pub const DEFAULT_FFT_N: usize = 6144;
pub const DEFAULT_INPUT_HOP: usize = 3072;

/// Compute the FFT size for given input + output rates so the FFT bin-
/// width matches the iFFT bin-width exactly. Both NB and WB iFFTs use
/// 62.5 Hz/bin, so the math is `fft_n = ifft_n × input_rate / output_rate`.
pub fn fft_n_for_rates(input_rate_hz: u32, output_rate_hz: u32) -> usize {
    let ifft_n = ifft_n_for_output_rate(output_rate_hz);
    let m = (input_rate_hz / output_rate_hz).max(1) as usize;
    let n = ifft_n.saturating_mul(m);
    if n % 2 == 0 { n } else { n + 1 }
}

/// Back-compat shim: assumes narrowband (8 kHz) output rate.
pub fn fft_n_for_rate(input_rate_hz: u32) -> usize {
    fft_n_for_rates(input_rate_hz, NB_OUTPUT_RATE_HZ)
}

/// Map output rate → iFFT size. Only NB / WB supported.
pub fn ifft_n_for_output_rate(output_rate_hz: u32) -> usize {
    match output_rate_hz {
        WB_OUTPUT_RATE_HZ => WB_IFFT_N,
        _ => NB_IFFT_N,
    }
}

/// Live FFT-channelizer for one VRX. Accumulates input I/Q samples,
/// processes complete FFT frames, returns narrowband audio at
/// `OUTPUT_RATE_HZ`.
pub struct VrxChannelizer {
    config: VrxConfig,
    input_rate_hz: u32,
    output_rate_hz: u32,

    // Runtime-derived FFT parameters (set in `new` from input_rate_hz
    // so FFT bin-width matches iFFT bin-width on any DDC rate).
    fft_n: usize,
    ifft_n: usize,
    input_hop: usize,
    output_hop: usize,

    // FFT/iFFT plans (Arc'd internally by rustfft)
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,

    // Hann analysis window
    window: Vec<f32>,

    // Accumulating input buffer; consumed in `input_hop`-sized chunks.
    input_buf: Vec<Complex32>,

    // FFT scratch buffers
    fft_buf: Vec<Complex32>,
    ifft_buf: Vec<Complex32>,

    // Overlap-add tail for the audio output. Complex so AM / SAM / FM
    // demod (non-linear) can combine overlapped frames before the
    // mode-specific demodulator runs. SSB modes just take .re.
    ola_tail: Vec<Complex32>,

    // AM envelope DC-removal filter state (running mean of |z|).
    am_dc_filter: f32,
    // FM previous-sample state for phase-diff demod.
    fm_last_sample: Complex32,

    // Residual NCO — compensates for the sub-bin frequency offset.
    // Bin-selection quantises to bin_width (typically 62.5 Hz). The
    // NCO rotates the complex baseband output after the iFFT so the
    // actual carrier lands exactly on the iFFT's zero frequency.
    nco_rotor: Complex32,                 // exp(-j 2π Δf / Fs_out) per sample
    nco_phase_at_frame_start: Complex32,
    samples_since_nco_renorm: u32,

    // AGC state
    agc_envelope: f32,
    agc_attack_alpha: f32,
    agc_decay_alpha: f32,
    agc_target: f32,
    agc_max_gain: f32,
}

impl VrxChannelizer {
    pub fn new(config: VrxConfig, input_rate_hz: u32) -> Self {
        Self::new_with_output_rate(config, input_rate_hz, NB_OUTPUT_RATE_HZ)
    }

    /// Construct with explicit output sample rate (8 kHz = NB, 16 kHz = WB).
    pub fn new_with_output_rate(
        config: VrxConfig,
        input_rate_hz: u32,
        output_rate_hz: u32,
    ) -> Self {
        let ifft_n = ifft_n_for_output_rate(output_rate_hz);
        let output_hop = ifft_n / 2;
        let fft_n = fft_n_for_rates(input_rate_hz, output_rate_hz);
        let input_hop = fft_n / 2; // 50% overlap

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_n);
        let ifft = planner.plan_fft_inverse(ifft_n);

        let window: Vec<f32> = (0..fft_n)
            .map(|i| {
                let x = i as f32 / (fft_n - 1) as f32;
                0.5 - 0.5 * (std::f32::consts::TAU * x).cos()
            })
            .collect();

        let t_sample = 1.0 / output_rate_hz as f32;
        let agc_attack_alpha = 1.0 - (-t_sample / 0.010).exp();
        let agc_decay_alpha = 1.0 - (-t_sample / 0.500).exp();

        // Compute residual offset: target carrier may not land exactly
        // on a bin centre. NCO rotates by -residual_hz per sample to
        // bring the carrier to true zero in the iFFT output.
        let bin_width = input_rate_hz as f32 / fft_n as f32;
        let nearest_bin = (config.carrier_offset_hz / bin_width).round();
        let residual_hz = config.carrier_offset_hz - nearest_bin * bin_width;
        let theta_per_sample =
            -std::f32::consts::TAU * residual_hz / output_rate_hz as f32;
        let nco_rotor = Complex32::from_polar(1.0, theta_per_sample);

        Self {
            config,
            input_rate_hz,
            output_rate_hz,
            fft_n,
            ifft_n,
            input_hop,
            output_hop,
            fft,
            ifft,
            window,
            input_buf: Vec::with_capacity(fft_n * 2),
            fft_buf: vec![Complex32::new(0.0, 0.0); fft_n],
            ifft_buf: vec![Complex32::new(0.0, 0.0); ifft_n],
            ola_tail: vec![Complex32::new(0.0, 0.0); output_hop],
            am_dc_filter: 0.0,
            fm_last_sample: Complex32::new(0.0, 0.0),
            nco_rotor,
            nco_phase_at_frame_start: Complex32::new(1.0, 0.0),
            samples_since_nco_renorm: 0,
            // Channelizer output is typically tiny (~1e-5) after bin-
            // selection on a wideband stream. Pulling that up to an
            // AGC target of 0.5 needs gain ~50 000, so the ceiling is
            // set to 100 000. The attack/decay envelope still limits
            // how fast the gain can climb on noise-only spans.
            agc_envelope: 0.00001,
            agc_attack_alpha,
            agc_decay_alpha,
            agc_target: 0.5,
            agc_max_gain: 100_000.0,
        }
    }

    pub fn output_rate(&self) -> u32 {
        self.output_rate_hz
    }

    pub fn ifft_size(&self) -> usize {
        self.ifft_n
    }

    pub fn config(&self) -> &VrxConfig {
        &self.config
    }

    /// Sample rate this channelizer was constructed for. Callers use
    /// this to detect input-rate changes and rebuild the channelizer
    /// with a matching FFT size.
    pub fn input_rate_hz(&self) -> u32 {
        self.input_rate_hz
    }

    /// Update SSB filter window (audio-bin offsets from carrier).
    /// Cheap mutation; safe to call every batch.
    pub fn set_filter(&mut self, lo_bins: usize, hi_bins: usize) {
        self.config.filter_lo_bins = lo_bins;
        self.config.filter_hi_bins = hi_bins;
    }

    /// Update the carrier offset (Hz from the IQ stream's centre).
    /// Called every batch when VFO/DDC change so the bin-selection
    /// follows the user's intended frequency.
    pub fn set_carrier_offset(&mut self, offset_hz: f32) {
        if (offset_hz - self.config.carrier_offset_hz).abs() < f32::EPSILON {
            return;
        }
        self.config.carrier_offset_hz = offset_hz;
        // Recompute residual NCO so sub-bin tuning is correct for the
        // new offset. We let the running `nco_phase_at_frame_start`
        // continue from where it was — a small phase discontinuity is
        // fine; resetting it would be worse (audible click).
        let bin_width = self.input_rate_hz as f32 / self.fft_n as f32;
        let nearest_bin = (offset_hz / bin_width).round();
        let residual_hz = offset_hz - nearest_bin * bin_width;
        let theta_per_sample =
            -std::f32::consts::TAU * residual_hz / self.output_rate_hz as f32;
        self.nco_rotor = Complex32::from_polar(1.0, theta_per_sample);
    }

    /// Feed a batch of input I/Q samples. Returns any audio samples
    /// produced from completed FFT frames (at `OUTPUT_RATE_HZ`).
    pub fn push_iq(&mut self, iq_pairs: &[(f32, f32)]) -> Vec<f32> {
        self.input_buf
            .extend(iq_pairs.iter().map(|&(i, q)| Complex32::new(i, q)));

        let mut audio_out: Vec<f32> = Vec::new();

        while self.input_buf.len() >= self.fft_n {
            // Window + load
            for i in 0..self.fft_n {
                self.fft_buf[i] = self.input_buf[i] * self.window[i];
            }
            self.fft.process(&mut self.fft_buf);

            // Bin-select → iFFT input (zero first, then fill)
            for slot in self.ifft_buf.iter_mut() {
                *slot = Complex32::new(0.0, 0.0);
            }
            let bin_width = self.input_rate_hz as f32 / self.fft_n as f32;
            let carrier_bin =
                (self.config.carrier_offset_hz / bin_width).round() as i32;
            let lo = self.config.filter_lo_bins.min(self.ifft_n / 2);
            let hi = self.config.filter_hi_bins.min(self.ifft_n / 2).max(lo);
            match self.config.mode {
                VrxMode::Usb => {
                    for i in lo..hi {
                        let bin = ((carrier_bin + i as i32)
                            .rem_euclid(self.fft_n as i32))
                            as usize;
                        self.ifft_buf[i] = self.fft_buf[bin];
                    }
                }
                VrxMode::Lsb => {
                    let start = (lo + 1).max(1);
                    let end = hi + 1;
                    for k in start..end {
                        let bin = ((carrier_bin - k as i32)
                            .rem_euclid(self.fft_n as i32))
                            as usize;
                        self.ifft_buf[self.ifft_n - k] = self.fft_buf[bin];
                    }
                }
                VrxMode::Am | VrxMode::Sam | VrxMode::Fm => {
                    // Both sidebands → complex baseband. Carrier bin at
                    // iFFT[0], USB at iFFT[1..hi], LSB at iFFT[N-1..N-hi+1].
                    if lo == 0 {
                        self.ifft_buf[0] = self.fft_buf
                            [(carrier_bin.rem_euclid(self.fft_n as i32)) as usize];
                    }
                    let start = lo.max(1);
                    for k in start..=hi {
                        let bin_up = ((carrier_bin + k as i32)
                            .rem_euclid(self.fft_n as i32))
                            as usize;
                        let bin_dn = ((carrier_bin - k as i32)
                            .rem_euclid(self.fft_n as i32))
                            as usize;
                        self.ifft_buf[k] = self.fft_buf[bin_up];
                        self.ifft_buf[self.ifft_n - k] = self.fft_buf[bin_dn];
                    }
                }
            }
            self.ifft.process(&mut self.ifft_buf);

            // Apply residual NCO to compensate sub-bin frequency offset.
            // Phase must stay coherent across overlap-add frames.
            let mut nco_phase = self.nco_phase_at_frame_start;
            for slot in self.ifft_buf.iter_mut() {
                *slot *= nco_phase;
                nco_phase *= self.nco_rotor;
            }
            for _ in 0..self.output_hop {
                self.nco_phase_at_frame_start *= self.nco_rotor;
            }
            // Periodically renormalize to keep magnitude ≈ 1.0
            // (floating-point drift accumulates over many multiplies).
            self.samples_since_nco_renorm += self.output_hop as u32;
            if self.samples_since_nco_renorm >= 1024 {
                let mag = self.nco_phase_at_frame_start.norm();
                if mag > 0.0 {
                    self.nco_phase_at_frame_start /= mag;
                }
                self.samples_since_nco_renorm = 0;
            }

            let norm = 1.0 / self.fft_n as f32;

            // Complex overlap-add → per-mode demod → AGC.
            for i in 0..self.output_hop {
                let z = self.ifft_buf[i] * norm + self.ola_tail[i];
                let sample = match self.config.mode {
                    VrxMode::Usb | VrxMode::Lsb => z.re,
                    VrxMode::Am => {
                        let mag = (z.re * z.re + z.im * z.im).sqrt();
                        // Slow DC tracker removes the carrier-amplitude bias.
                        self.am_dc_filter = 0.999 * self.am_dc_filter + 0.001 * mag;
                        mag - self.am_dc_filter
                    }
                    VrxMode::Sam => {
                        // Coherent product-detector: in-phase component of
                        // carrier-aligned baseband. Subtract the same DC
                        // tracker so the carrier doesn't dominate the AGC.
                        self.am_dc_filter = 0.999 * self.am_dc_filter + 0.001 * z.re;
                        z.re - self.am_dc_filter
                    }
                    VrxMode::Fm => {
                        // FM discriminator: phase difference between
                        // consecutive complex baseband samples.
                        let prod = z * self.fm_last_sample.conj();
                        self.fm_last_sample = z;
                        prod.im.atan2(prod.re)
                    }
                };
                let abs_in = sample.abs();
                let alpha = if abs_in > self.agc_envelope {
                    self.agc_attack_alpha
                } else {
                    self.agc_decay_alpha
                };
                self.agc_envelope += alpha * (abs_in - self.agc_envelope);
                let gain = (self.agc_target / self.agc_envelope.max(1e-6))
                    .min(self.agc_max_gain);
                audio_out.push(sample * gain);
            }
            for i in 0..self.output_hop {
                self.ola_tail[i] = self.ifft_buf[i + self.output_hop] * norm;
            }

            // Slide the input buffer by input_hop (= 50% overlap).
            self.input_buf.drain(..self.input_hop);
        }

        audio_out
    }
}
