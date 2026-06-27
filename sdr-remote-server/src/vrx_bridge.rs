// SPDX-License-Identifier: GPL-2.0-or-later

//! ThetisLink adapter rond de generieke `vrx-rs` crate.
//!
//! Maps the standalone runtime's outputs to ThetisLink-specific
//! transport (VrxAudioPacket via UDP) and provides a process-wide
//! singleton control-state per VRX channel.
//!
//! Owner of all ThetisLink-VRX coupling. If `vrx-rs` is ever
//! removed from this server, only this file + the dependency in
//! `Cargo.toml` need to go.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::net::UdpSocket;

use sdr_remote_core::protocol::{PacketType, VrxAudioPacket, VrxFrequencyPacket};
use sdr_remote_core::MAX_PACKET_SIZE;
use vrx_rs::{VrxAudioCallback, VrxControlState};

/// Process-wide VRX control state per channel. `id=0` is VRX1 (RX1 IQ
/// stream + VFO-A), `id=1` is VRX2 (RX2 IQ stream + VFO-B). Both
/// channels are lazily created at first access; out-of-range ids fall
/// back to id=0 to keep callers panic-free.
///
/// Sync `std::sync::Mutex` — lock-windows are short (lees enable/
/// freq/mode/volume), nooit een `.await` binnen de lock. ThetisLink's
/// async network handler en het audio-loop locken beide sync.
pub fn vrx_control_thetislink(id: u8) -> Arc<Mutex<VrxControlState>> {
    static SLOT: OnceLock<Vec<Arc<Mutex<VrxControlState>>>> = OnceLock::new();
    let v = SLOT.get_or_init(|| {
        vec![
            Arc::new(Mutex::new(VrxControlState::default())),
            Arc::new(Mutex::new(VrxControlState::default())),
        ]
    });
    let idx = (id as usize).min(v.len() - 1);
    v[idx].clone()
}

/// `VrxAudioCallback` implementatie die per Opus-frame een
/// `VrxAudioPacket` opbouwt en naar elk actief client-adres stuurt
/// via `socket.try_send_to()`. `addrs` is een snapshot dat de caller
/// vóór elke `feed()` aanroep ververst onder de session lock (sync
/// `try_send_to` heeft geen async runtime nodig).
pub struct ThetisVrxSink {
    pub socket: Arc<UdpSocket>,
    pub addrs: Vec<SocketAddr>,
    /// SAM auto-tune subscribers (per-client gated on `VrxSamAutoTune*`).
    /// Only these receive `FrequencyVrxActual` — old clients never do.
    pub autotune_addrs: Vec<SocketAddr>,
    pub timestamp_ms: u32,
    pub buf: Vec<u8>,
    /// Tags every emitted VrxAudioPacket as wideband (16 kHz Opus) vs.
    /// narrowband (8 kHz). Owner sets this before each feed() based on
    /// per-session toggle state.
    pub wideband: bool,
}

impl ThetisVrxSink {
    pub fn new(socket: Arc<UdpSocket>) -> Self {
        Self {
            socket,
            addrs: Vec::new(),
            autotune_addrs: Vec::new(),
            timestamp_ms: 0,
            buf: Vec::with_capacity(MAX_PACKET_SIZE),
            wideband: false,
        }
    }
}

impl VrxAudioCallback for ThetisVrxSink {
    fn on_frame(&mut self, vrx_id: u8, _audio: &[f32], opus_bytes: &[u8], sequence: u32) {
        if self.addrs.is_empty() || opus_bytes.is_empty() {
            return;
        }
        let packet = VrxAudioPacket {
            sequence,
            timestamp: self.timestamp_ms,
            vrx_id,
            opus_data: opus_bytes.to_vec(),
            wideband: self.wideband,
        };
        self.buf.clear();
        packet.serialize(&mut self.buf);
        for addr in &self.addrs {
            let _ = self.socket.try_send_to(&self.buf, *addr);
        }
    }

    fn on_carrier_freq(&mut self, vrx_id: u8, freq_hz: u64) {
        if self.autotune_addrs.is_empty() {
            return;
        }
        let pkt = VrxFrequencyPacket { vrx_id, frequency_hz: freq_hz };
        let mut buf = [0u8; VrxFrequencyPacket::SIZE];
        pkt.serialize_with_type(PacketType::FrequencyVrxActual, &mut buf);
        for addr in &self.autotune_addrs {
            let _ = self.socket.try_send_to(&buf, *addr);
        }
    }
}
