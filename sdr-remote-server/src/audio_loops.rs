// SPDX-License-Identifier: GPL-2.0-or-later

//! Audio encoding/sending loops extracted from network.rs.
//! Provides multi-channel + Yaesu audio bundlers and an IQ consumer loop.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use log::{info, warn};
use tokio::net::UdpSocket;
use tokio::sync::{watch, Mutex};
use tokio::time::{interval, Duration};

use sdr_remote_core::codec::OpusEncoder;
use sdr_remote_core::protocol::*;
use sdr_remote_core::{FRAME_SAMPLES, MAX_PACKET_SIZE, NETWORK_SAMPLE_RATE};

use crate::ptt::PttController;
use crate::session::SessionManager;

// â”€â”€ Resampling helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Resample i16 8kHz â†’ f32 device rate
pub fn resample_to_device(resampler: &mut impl rubato::Resampler<f32>, pcm_i16: &[i16]) -> Vec<f32> {
    let input_f32: Vec<f32> = pcm_i16.iter().map(|&s| s as f32 / 32768.0).collect();
    match resampler.process(&[input_f32], None) {
        Ok(result) => result.into_iter().next().unwrap_or_default(),
        Err(e) => {
            warn!("resample 8kâ†’device error: {}", e);
            Vec::new()
        }
    }
}

/// Resample f32 device rate â†’ f32 8kHz
pub fn resample_to_network(resampler: &mut impl rubato::Resampler<f32>, pcm_f32: &[f32]) -> Vec<f32> {
    match resampler.process(&[pcm_f32.to_vec()], None) {
        Ok(result) => result.into_iter().next().unwrap_or_default(),
        Err(e) => {
            warn!("resample deviceâ†’8k error: {}", e);
            Vec::new()
        }
    }
}

/// Standard high-quality sinc resampler parameters (used by server audio loops)
pub fn hq_sinc_params() -> rubato::SincInterpolationParameters {
    rubato::SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: rubato::SincInterpolationType::Cubic,
        window: rubato::WindowFunction::Blackman,
    }
}

// ── Multi-channel audio bundler ─────────────────────────────────────────

/// Multi-channel audio loop that replaces the three separate TCI loops.
/// Always sends L=RX1 (or RX1-L when BIN), R=RX2 (or RX1-R when BIN).
/// The client decides how to play L and R (mono/split/binaural).
pub async fn tci_multichannel_audio_loop(
    socket: Arc<UdpSocket>,
    session: Arc<Mutex<SessionManager>>,
    ptt: Arc<Mutex<PttController>>,
    mut rx1_audio_rx: Option<tokio::sync::mpsc::Receiver<Vec<f32>>>,
    mut rx2_audio_rx: Option<tokio::sync::mpsc::Receiver<Vec<f32>>>,
    mut bin_r_audio_rx: Option<tokio::sync::mpsc::Receiver<Vec<f32>>>,
    shutdown: &mut watch::Receiver<bool>,
    start: Instant,
    audio_stats: Arc<crate::audio_stats::AudioActivityStats>,
    server_start: Instant,
) -> Result<()> {
    let tci_rate = 48000u32;
    let tci_frame_samples = (tci_rate * 20 / 1000) as usize; // 960

    // Per-channel mono encoders
    let mut enc_rx1 = OpusEncoder::new()?;
    let mut enc_bin_r = OpusEncoder::new()?;
    let mut enc_rx2 = OpusEncoder::new()?;

    // Per-channel mono resamplers: 48kHz → 8kHz
    let mk_resampler = || rubato::SincFixedIn::<f32>::new(
        NETWORK_SAMPLE_RATE as f64 / tci_rate as f64, 1.0,
        hq_sinc_params(), tci_frame_samples, 1,
    );
    let mut res_rx1 = mk_resampler().context("RX1 resampler")?;
    let mut res_bin_r = mk_resampler().context("BinR resampler")?;
    let mut res_rx2 = mk_resampler().context("RX2 resampler")?;

    let mut sequence: u32 = 0;
    let mut rx1_accum: Vec<f32> = Vec::with_capacity(tci_frame_samples * 4);
    let mut rx2_accum: Vec<f32> = Vec::with_capacity(tci_frame_samples * 4);
    let mut bin_r_accum: Vec<f32> = Vec::with_capacity(tci_frame_samples * 4);
    let mut tick = interval(Duration::from_millis(20));
    let mut had_clients = false;

    info!("Stereo audio mixer started");

    loop {
        // Try to acquire missing channels
        if rx1_audio_rx.is_none() || rx2_audio_rx.is_none() || bin_r_audio_rx.is_none() {
            let mut ptt_guard = ptt.lock().await;
            if let Some(tci) = Some(&mut ptt_guard.tci) {
                if rx1_audio_rx.is_none() { rx1_audio_rx = tci.rx1_audio_rx.take(); }
                if rx2_audio_rx.is_none() { rx2_audio_rx = tci.rx2_audio_rx.take(); }
                if bin_r_audio_rx.is_none() { bin_r_audio_rx = tci.bin_r_audio_rx.take(); }
            }
            drop(ptt_guard);
            if rx1_audio_rx.is_none() {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => continue,
                    _ = shutdown.changed() => break,
                }
            }
        }

        tokio::select! {
            // Wait for tick or shutdown â€” audio is drained non-blocking below
            _ = tick.tick() => {
                // Drain ALL channels non-blocking to prevent select! bias
                fn drain_channel(rx_opt: &mut Option<tokio::sync::mpsc::Receiver<Vec<f32>>>, accum: &mut Vec<f32>) {
                    if let Some(rx) = rx_opt.as_mut() {
                        loop {
                            match rx.try_recv() {
                                Ok(s) => accum.extend_from_slice(&s),
                                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                    *rx_opt = None;
                                    accum.clear();
                                    break;
                                }
                            }
                        }
                    }
                }
                drain_channel(&mut rx1_audio_rx, &mut rx1_accum);
                drain_channel(&mut rx2_audio_rx, &mut rx2_accum);
                drain_channel(&mut bin_r_audio_rx, &mut bin_r_accum);
                // Cap accumulators
                let max = tci_frame_samples * 10;
                if rx1_accum.len() > max { rx1_accum.drain(..rx1_accum.len() - max); }
                if rx2_accum.len() > max { rx2_accum.drain(..rx2_accum.len() - max); }
                if bin_r_accum.len() > max { bin_r_accum.drain(..bin_r_accum.len() - max); }
                if rx1_accum.len() < tci_frame_samples {
                    continue;
                }

                let addrs = {
                    let sess = session.lock().await;
                    sess.active_addrs()
                };
                let has_clients = !addrs.is_empty();
                if !has_clients {
                    had_clients = false;
                    continue;
                }

                // Align accumulators on first tick or when a client (re)connects
                if !had_clients {
                    info!("Multi-ch audio: client connected, aligning accumulators (rx1={} rx2={} binr={})",
                        rx1_accum.len(), rx2_accum.len(), bin_r_accum.len());
                    if rx1_accum.len() > tci_frame_samples {
                        rx1_accum.drain(..rx1_accum.len() - tci_frame_samples);
                    }
                    if rx2_accum.len() > tci_frame_samples {
                        rx2_accum.drain(..rx2_accum.len() - tci_frame_samples);
                    }
                    if bin_r_accum.len() > tci_frame_samples {
                        bin_r_accum.drain(..bin_r_accum.len() - tci_frame_samples);
                    }
                    had_clients = true;
                }

                // Encode each available channel as mono Opus and bundle
                let mut channels: Vec<(u8, Vec<u8>)> = Vec::with_capacity(3);

                // CH0: RX1 (always present)
                let rx1_frame: Vec<f32> = rx1_accum.drain(..tci_frame_samples).collect();
                let rx1_8k = resample_to_network(&mut res_rx1, &rx1_frame);
                let rx1_i16: Vec<i16> = rx1_8k.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16).collect();
                if rx1_i16.len() >= FRAME_SAMPLES {
                    if let Ok(opus) = enc_rx1.encode(&rx1_i16[..FRAME_SAMPLES]) {
                        channels.push((0, opus)); // CH_RX1
                        audio_stats.rx1.tick(server_start);
                    }
                }

                // CH1: BinR (only when Thetis binaural active)
                if bin_r_accum.len() >= tci_frame_samples {
                    let frame: Vec<f32> = bin_r_accum.drain(..tci_frame_samples).collect();
                    let pcm_8k = resample_to_network(&mut res_bin_r, &frame);
                    let i16s: Vec<i16> = pcm_8k.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16).collect();
                    if i16s.len() >= FRAME_SAMPLES {
                        if let Ok(opus) = enc_bin_r.encode(&i16s[..FRAME_SAMPLES]) {
                            channels.push((1, opus)); // CH_BIN_R
                        }
                    }
                }

                // CH2: RX2 (when RX2 audio available)
                if rx2_accum.len() >= tci_frame_samples {
                    let frame: Vec<f32> = rx2_accum.drain(..tci_frame_samples).collect();
                    let pcm_8k = resample_to_network(&mut res_rx2, &frame);
                    let i16s: Vec<i16> = pcm_8k.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16).collect();
                    if i16s.len() >= FRAME_SAMPLES {
                        if let Ok(opus) = enc_rx2.encode(&i16s[..FRAME_SAMPLES]) {
                            channels.push((2, opus)); // CH_RX2
                            audio_stats.rx2.tick(server_start);
                        }
                    }
                }

                // Drain excess accumulators
                if bin_r_accum.len() > tci_frame_samples {
                    bin_r_accum.drain(..bin_r_accum.len() - tci_frame_samples);
                }
                if rx2_accum.len() > tci_frame_samples {
                    rx2_accum.drain(..rx2_accum.len() - tci_frame_samples);
                }

                // Send per-client filtered multi-channel packets
                if !channels.is_empty() {
                    let timestamp = start.elapsed().as_millis() as u32;
                    // Read per-client modes + rx2_enabled flag under short
                    // lock, then release. `rx2_enabled` gates CH2 even when
                    // `audio_mode` would otherwise allow it — the desktop
                    // client UI's "RX2 enabled" toggle must mute the
                    // upstream RX2 stream entirely, not just the local
                    // playback (bandwidth bug uncovered 2026-05-13).
                    let client_modes: Vec<(std::net::SocketAddr, u8, bool)> = {
                        let sess = session.lock().await;
                        addrs
                            .iter()
                            .map(|&a| (a, sess.client_audio_mode(a), sess.client_rx2_enabled(a)))
                            .collect()
                    };

                    for (addr, mode, rx2_enabled) in &client_modes {
                        // Filter channels based on client's audio mode.
                        // Then drop CH2 (RX2) for clients that have RX2
                        // turned off — those bytes would otherwise reach
                        // the client and be silently mixed into mono
                        // output (or burn data on metered links).
                        // mode 255 (default/Android): CH0 only
                        // mode 0 (Mono): CH0 + CH2  (gated by rx2_enabled)
                        // mode 1 (BIN): CH0 + CH1 + CH2  (CH2 gated)
                        // mode 2 (Split): CH0 + CH2  (CH2 gated)
                        let client_chs: Vec<(u8, Vec<u8>)> = channels.iter()
                            .filter(|(ch_id, _)| {
                                let allowed = match *mode {
                                    255 => *ch_id == 0,                    // Android: RX1 only
                                    0 => *ch_id == 0 || *ch_id == 2,      // Mono: RX1 + RX2
                                    1 => true,                             // BIN: all
                                    2 => *ch_id == 0 || *ch_id == 2,      // Split: RX1 + RX2
                                    _ => *ch_id == 0,
                                };
                                if !allowed { return false; }
                                if *ch_id == 2 && !rx2_enabled { return false; }
                                true
                            })
                            .cloned()
                            .collect();

                        if !client_chs.is_empty() {
                            let packet = sdr_remote_core::protocol::MultiChannelAudioPacket {
                                sequence,
                                timestamp,
                                channels: client_chs,
                            };
                            let mut send_buf = Vec::with_capacity(MAX_PACKET_SIZE);
                            packet.serialize(&mut send_buf);
                            let _ = socket.send_to(&send_buf, addr).await;
                        }
                    }
                    sequence = sequence.wrapping_add(1);
                }
            }
            _ = shutdown.changed() => break,
        }
    }

    info!("Multi-channel audio bundler stopped");
    Ok(())
}

// â”€â”€ Yaesu audio loop â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Yaesu USB audio TX loop: receives from cpal, encodes Opus, sends to clients.
pub async fn yaesu_audio_loop(
    socket: Arc<UdpSocket>,
    session: Arc<Mutex<SessionManager>>,
    mut audio_rx: tokio::sync::mpsc::Receiver<Vec<f32>>,
    sample_rate: u32,
    shutdown: &mut watch::Receiver<bool>,
    start: Instant,
    audio_stats: Arc<crate::audio_stats::AudioActivityStats>,
    server_start: Instant,
) -> Result<()> {
    let frame_samples = (sample_rate * 20 / 1000) as usize;

    let mut encoder = OpusEncoder::new()?;
    let mut resampler_in = rubato::SincFixedIn::<f32>::new(
        NETWORK_SAMPLE_RATE as f64 / sample_rate as f64,
        1.0,
        hq_sinc_params(),
        frame_samples,
        1,
    ).context("create Yaesu audio resampler")?;

    let mut sequence: u32 = 0;
    let mut accumulator: Vec<f32> = Vec::with_capacity(frame_samples * 4);
    let mut tick = interval(Duration::from_millis(20));
    let mut had_clients = false;

    info!("Yaesu audio TX loop started ({}Hz â†’ {}Hz Opus)", sample_rate, NETWORK_SAMPLE_RATE);

    loop {
        tokio::select! {
            result = audio_rx.recv() => {
                match result {
                    Some(samples) => {
                        accumulator.extend_from_slice(&samples);
                        let max_accum = frame_samples * 10;
                        if accumulator.len() > max_accum {
                            accumulator.drain(..accumulator.len() - max_accum);
                        }
                    }
                    None => {
                        info!("Yaesu audio channel closed");
                        break;
                    }
                }
            }
            _ = tick.tick() => {
                let addrs = session.lock().await.yaesu_addrs();
                if addrs.is_empty() {
                    accumulator.clear();
                    had_clients = false;
                    continue;
                }

                if !had_clients {
                    match OpusEncoder::new() {
                        Ok(new_enc) => {
                            encoder = new_enc;
                            sequence = 0;
                            accumulator.clear();
                            had_clients = true;
                            info!("Yaesu audio: client(s) enabled, encoder reset");
                        }
                        Err(e) => {
                            log::error!("Yaesu encoder reset failed: {} — Yaesu audio TX skipped this tick (server blijft draaien)", e);
                            // had_clients stays false → retry next tick if clients still present.
                        }
                    }
                    continue;
                }

                if accumulator.len() < frame_samples {
                    continue;
                }
                let frame: Vec<f32> = accumulator.drain(..frame_samples).collect();

                let pcm_8k = resample_to_network(&mut resampler_in, &frame);
                let pcm_i16: Vec<i16> = pcm_8k.iter()
                    .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                    .collect();

                if pcm_i16.len() >= FRAME_SAMPLES {
                    let opus_data = encoder.encode(&pcm_i16[..FRAME_SAMPLES])?;
                    let timestamp = start.elapsed().as_millis() as u32;
                    let packet = AudioPacket {
                        flags: Flags::NONE,
                        sequence,
                        timestamp,
                        opus_data,
                    };
                    sequence = sequence.wrapping_add(1);

                    let mut send_buf = Vec::with_capacity(MAX_PACKET_SIZE);
                    packet.serialize_as_type(&mut send_buf, PacketType::AudioYaesu);

                    for &addr in &addrs {
                        let _ = socket.send_to(&send_buf, addr).await;
                    }
                    audio_stats.yaesu_rx.tick(server_start);
                }
            }
            _ = shutdown.changed() => break,
        }
    }

    Ok(())
}

// â”€â”€ TCI IQ consumer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Drains IQ channels from TCI and feeds spectrum processors (RX1 + RX2).
pub async fn tci_iq_consumer(
    ptt: Arc<Mutex<PttController>>,
    spectrum: Arc<Mutex<crate::spectrum::SpectrumProcessor>>,
    rx2_spectrum: Arc<Mutex<crate::spectrum::Rx2SpectrumProcessor>>,
    shutdown: &mut watch::Receiver<bool>,
) {
    let mut iq_rx1: Option<tokio::sync::mpsc::Receiver<(u32, Vec<(f32, f32)>)>> = None;
    let mut iq_rx2: Option<tokio::sync::mpsc::Receiver<(u32, Vec<(f32, f32)>)>> = None;

    let mut fft_size = spectrum.lock().await.ddc_fft_size();
    let mut rx2_fft_size = rx2_spectrum.lock().await.ddc_fft_size();
    let mut hop_size = sdr_remote_core::ddc_hop_size(fft_size);
    let mut rx2_hop_size = sdr_remote_core::ddc_hop_size(rx2_fft_size);
    let mut rx1_accum: Vec<(f32, f32)> = Vec::with_capacity(fft_size * 2);
    let mut rx2_accum: Vec<(f32, f32)> = Vec::with_capacity(rx2_fft_size * 2);
    let mut rx1_iq_rate: u32 = 0; // Detected from RX1 IQ frame headers
    let mut rx2_iq_rate: u32 = 0; // Detected from RX2 IQ frame headers (can differ from RX1)

    loop {
        if iq_rx1.is_none() || iq_rx2.is_none() {
            let mut ptt_guard = ptt.lock().await;
            if let Some(tci) = Some(&mut ptt_guard.tci) {
                if iq_rx1.is_none() { iq_rx1 = tci.iq_rx1_rx.take(); }
                if iq_rx2.is_none() { iq_rx2 = tci.iq_rx2_rx.take(); }
            }
            drop(ptt_guard);
            if iq_rx1.is_none() && iq_rx2.is_none() {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => continue,
                    _ = shutdown.changed() => break,
                }
            }
        }

        tokio::select! {
            result = async {
                if let Some(rx) = iq_rx1.as_mut() { rx.recv().await } else { std::future::pending().await }
            } => {
                let (frame_rate, iq_pairs) = match result {
                    Some(p) => p,
                    None => {
                        iq_rx1 = None;
                        rx1_accum.clear();
                        continue;
                    }
                };
                // Dynamic IQ sample rate detection from RX1 binary frame header
                if frame_rate != rx1_iq_rate && frame_rate > 0 {
                    info!("TCI RX1 IQ sample rate: {}kHz (was {}kHz)",
                        frame_rate / 1000, if rx1_iq_rate > 0 { rx1_iq_rate / 1000 } else { 0 });
                    rx1_iq_rate = frame_rate;
                    spectrum.lock().await.update_sample_rate(frame_rate);
                    fft_size = spectrum.lock().await.ddc_fft_size();
                    hop_size = sdr_remote_core::ddc_hop_size(fft_size);
                    rx1_accum.clear();
                }
                rx1_accum.extend_from_slice(&iq_pairs);
                let cur_fft = spectrum.lock().await.ddc_fft_size();
                if cur_fft != fft_size {
                    fft_size = cur_fft;
                    hop_size = sdr_remote_core::ddc_hop_size(fft_size);
                    rx1_accum.clear();
                }
                while rx1_accum.len() >= fft_size {
                    let frame: Vec<(f32, f32)> = rx1_accum[..fft_size].to_vec();
                    rx1_accum.drain(..hop_size);
                    spectrum.lock().await.process_ddc_frame(&frame);
                    tokio::task::yield_now().await;
                }
            }
            result = async {
                if let Some(rx) = iq_rx2.as_mut() { rx.recv().await } else { std::future::pending().await }
            } => {
                let (frame_rate, iq_pairs) = match result {
                    Some(p) => p,
                    None => {
                        iq_rx2 = None;
                        rx2_accum.clear();
                        continue;
                    }
                };
                // Dynamic IQ sample rate detection from RX2 binary frame header
                if frame_rate != rx2_iq_rate && frame_rate > 0 {
                    info!("TCI RX2 IQ sample rate: {}kHz (was {}kHz)",
                        frame_rate / 1000, if rx2_iq_rate > 0 { rx2_iq_rate / 1000 } else { 0 });
                    rx2_iq_rate = frame_rate;
                    rx2_spectrum.lock().await.update_sample_rate(frame_rate);
                    rx2_fft_size = rx2_spectrum.lock().await.ddc_fft_size();
                    rx2_hop_size = sdr_remote_core::ddc_hop_size(rx2_fft_size);
                    rx2_accum.clear();
                }
                rx2_accum.extend_from_slice(&iq_pairs);
                let cur_fft = rx2_spectrum.lock().await.ddc_fft_size();
                if cur_fft != rx2_fft_size {
                    rx2_fft_size = cur_fft;
                    rx2_hop_size = sdr_remote_core::ddc_hop_size(rx2_fft_size);
                    rx2_accum.clear();
                }
                while rx2_accum.len() >= rx2_fft_size {
                    let frame: Vec<(f32, f32)> = rx2_accum[..rx2_fft_size].to_vec();
                    rx2_accum.drain(..rx2_hop_size);
                    rx2_spectrum.lock().await.process_ddc_frame(&frame);
                    tokio::task::yield_now().await;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)), if iq_rx1.is_none() || iq_rx2.is_none() => {
                continue;
            }
            _ = shutdown.changed() => break,
        }
    }
}
