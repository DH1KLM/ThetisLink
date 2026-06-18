// SPDX-License-Identifier: GPL-2.0-or-later

//! Channelizer + runtime configuration types. Designed to be cheap
//! to clone and shareable across threads via `Arc<Mutex<...>>`.

/// Audio-bin width at output rate (8 kHz / 128-point iFFT = 62.5 Hz).
/// Filter edges (in Hz) are quantised to multiples of this so the
/// channelizer can map them straight to bin indices without aliasing.
pub const AUDIO_BIN_HZ: f32 = 62.5;

/// Demodulation mode for the channelizer.
///
/// SSB modes (Usb/Lsb) extract a single sideband and emit Re(z).
/// AM-family modes (Am/Sam) extract both sidebands and demodulate
/// either by envelope (Am) or coherent in-phase (Sam, assumes user
/// is tuned on carrier — no PLL tracking yet).
/// Fm extracts both sidebands and emits the phase derivative
/// between successive complex baseband samples.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VrxMode {
    Usb,
    Lsb,
    Am,
    Sam,
    Fm,
}

impl VrxMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "usb" => Some(VrxMode::Usb),
            "lsb" => Some(VrxMode::Lsb),
            "am" => Some(VrxMode::Am),
            "sam" => Some(VrxMode::Sam),
            "fm" => Some(VrxMode::Fm),
            _ => None,
        }
    }
}

/// Channelizer fixed configuration (carrier offset + mode + bandwidth).
/// `carrier_offset_hz` is dynamic (updated each batch via
/// `VrxChannelizer::set_carrier_offset`); the other fields are
/// effectively const after construction.
#[derive(Clone, Debug)]
pub struct VrxConfig {
    pub carrier_offset_hz: f32,
    pub mode: VrxMode,
    /// Audio-bin offsets describing the SSB filter window after demod.
    /// Both are positive distances from carrier in 62.5 Hz steps.
    /// `lo` is the nearer edge, `hi` the farther edge. For symmetric
    /// 3 kHz SSB: lo=0, hi=48.
    pub filter_lo_bins: usize,
    pub filter_hi_bins: usize,
}

impl Default for VrxConfig {
    fn default() -> Self {
        Self {
            carrier_offset_hz: 5000.0,
            mode: VrxMode::Usb,
            filter_lo_bins: 0,
            filter_hi_bins: 48,
        }
    }
}

/// Shared VRX control state — what an external UI/network layer
/// mutates and the runtime reads each IQ batch. Held by the caller
/// in an `Arc<std::sync::Mutex<VrxControlState>>`; lock-windows are
/// short (a handful of field reads/writes) and never include an
/// `.await`, so a sync mutex is the right primitive.
#[derive(Debug, Clone)]
pub struct VrxControlState {
    /// User-toggleable stream enable (default off). When false the
    /// runtime skips all DSP work and the callback is not invoked.
    pub enabled: bool,
    /// Absolute listen frequency in Hz. The runtime translates this
    /// to a bin-offset relative to the current DDC center each batch.
    /// 0 = "not set yet"; in that case the runtime falls back to
    /// the VFO frequency supplied to `feed()`.
    pub target_freq_hz: u64,
    /// Demodulation mode (USB / LSB).
    pub mode: VrxMode,
    /// Mix gain (1.0 = unity). Provided here as a shared field so
    /// the sink can apply it externally; the runtime itself does
    /// NOT apply this gain (audio output is post-AGC, pre-volume).
    pub volume: f32,
    /// SSB filter edges as signed offsets from carrier in Hz. The
    /// runtime quantises these to audio bins per current mode. For
    /// USB both edges are typically positive (0..=3000); for LSB
    /// both negative (-3000..=0).
    pub filter_low_hz: i32,
    pub filter_high_hz: i32,
}

impl Default for VrxControlState {
    fn default() -> Self {
        Self {
            enabled: false,
            target_freq_hz: 0,
            mode: VrxMode::Usb,
            volume: 1.0,
            filter_low_hz: 0,
            filter_high_hz: 3000,
        }
    }
}
