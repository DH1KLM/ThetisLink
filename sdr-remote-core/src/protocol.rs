// SPDX-License-Identifier: GPL-2.0-or-later

use anyhow::{bail, Result};
use num_enum::TryFromPrimitive;

/// Magic byte identifying our protocol
pub const MAGIC: u8 = 0xAA;

/// Protocol version.
///
/// **Breaking change v0.3.x → v2.0.0 (TL2):** new packet types (AudioRx2,
/// AudioYaesu, AudioBinR, AudioMultiCh, Diversity*, Yaesu*, etc.), expanded
/// ControlId range, new auth path. v1-clients cannot interoperate with v2-
/// servers and vice versa; mismatch surfaces as `unsupported version` at
/// header-deserialize time (early, explicit failure).
///
/// **Breaking change v2 → v3 (build 58 bundle):** `SmeterPacket.level`
/// reinterpreted from unsigned 0-260 display-units to signed deci-units
/// (dBm × 10 in RX, watts × 10 in TX). Wire size unchanged (2 bytes BE),
/// but a v2-peer reading a v3-packet (or vice versa) would clamp/overflow
/// silently and corrupt the S-meter. Bumping VERSION forces explicit
/// "unsupported version" rejection at handshake / first packet, so mixed
/// deploys fail loudly instead of producing garbage readings. Server,
/// desktop client and Android must all upgrade in lockstep.
pub const VERSION: u8 = 3;

/// Packet type identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum PacketType {
    Audio = 0x01,
    Heartbeat = 0x02,
    HeartbeatAck = 0x03,
    Control = 0x04,
    Disconnect = 0x05,
    PttDenied = 0x06,
    Frequency = 0x07,
    Mode = 0x08,
    Smeter = 0x09,
    Spectrum = 0x0A,
    /// Full DDC spectrum row for waterfall history (same format as Spectrum)
    FullSpectrum = 0x0B,
    /// Equipment status (server → client)
    EquipmentStatus = 0x0C,
    /// Equipment command (client → server)
    EquipmentCommand = 0x0D,
    /// RX2 audio (server → client, same format as Audio)
    AudioRx2 = 0x0E,
    /// RX2 frequency (bidirectional, same format as Frequency)
    FrequencyRx2 = 0x0F,
    /// RX2 mode (bidirectional, same format as Mode)
    ModeRx2 = 0x10,
    /// RX2 S-meter (server → client, same format as Smeter)
    SmeterRx2 = 0x11,
    /// RX2 spectrum (server → client, same format as Spectrum)
    SpectrumRx2 = 0x12,
    /// RX2 full spectrum row for waterfall (same format as FullSpectrum)
    FullSpectrumRx2 = 0x13,
    /// DX cluster spot (server → client)
    Spot = 0x14,
    /// TX profile list with names (server → client)
    TxProfiles = 0x15,
    /// Yaesu audio (server → client, same format as Audio)
    AudioYaesu = 0x16,
    /// Yaesu radio state (server → client)
    YaesuState = 0x17,
    /// Yaesu frequency set (client → server, same format as Frequency)
    FrequencyYaesu = 0x18,
    /// Binaural right channel audio (server → client, same format as Audio) [deprecated]
    AudioBinR = 0x1A,
    /// Multi-channel audio: 1-4 mono Opus frames bundled in one packet
    AudioMultiCh = 0x1B,
    /// RX1 S-meter Sig source (server → client, same SmeterPacket format).
    /// Emitted when client subscribes via SmeterSources bit 0. Carries WDSP
    /// RXA_S_PK (peak-hold with 100ms decay) — Multimeter "Sig" mode.
    SmeterSig = 0x1C,
    /// RX1 S-meter MaxBin source (server → client, same SmeterPacket format).
    /// Emitted when client subscribes via SmeterSources bit 2. Carries the
    /// highest single FFT bin in the passband — Multimeter "Max Bin" mode.
    SmeterMaxBin = 0x1D,
    /// RX2 S-meter Sig source (server → client). Bit 4 in SmeterSources.
    SmeterRx2Sig = 0x1E,
    /// RX2 S-meter MaxBin source (server → client). Bit 6 in SmeterSources.
    SmeterRx2MaxBin = 0x1F,
    /// Yaesu memory data (server → client, tab-separated text)
    YaesuMemoryData = 0x19,
    /// Amplitec power-cap table (server → client push, client → server update).
    /// 18-byte payload: 6 × { u16 max_w BE (0 = no cap), u8 tx_blocked (0/1) }.
    /// Drives the "Power-cap table" section in the client's Amplitec tab.
    AmplitecPowerTable = 0x20,
    /// Authentication challenge (server → client, 16-byte nonce)
    AuthChallenge = 0x30,
    /// Authentication response (client → server, 32-byte HMAC)
    AuthResponse = 0x31,
    /// Authentication result (server → client, 1 byte: 0=rejected, 1=accepted, 2=totp_required)
    AuthResult = 0x32,
    /// TOTP challenge (server → client, signals that TOTP code is needed)
    TotpChallenge = 0x33,
    /// TOTP response (client → server, 6-digit code as UTF-8 string)
    TotpResponse = 0x34,
}

impl PacketType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::try_from(v).ok()
    }
}

/// Control command identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum ControlId {
    /// RX1 AF gain (value 0-100, CAT: ZZLA)
    Rx1AfGain = 0x01,
    /// Power on/off (value 0/1, CAT: ZZPS)
    PowerOnOff = 0x02,
    /// TX Profile index (value 0-99, CAT: ZZTP)
    TxProfile = 0x03,
    /// Noise Reduction level (value 0-4: 0=off, 1=NR1, 2=NR2, 3=NR3, 4=NR4, CAT: ZZNE)
    NoiseReduction = 0x04,
    /// Auto Notch Filter on/off (value 0/1, CAT: ZZNT)
    AutoNotchFilter = 0x05,
    /// Drive level (value 0-100, CAT: ZZPC)
    DriveLevel = 0x06,
    /// Spectrum enable (value 0=off, 1=on)
    SpectrumEnable = 0x07,
    /// Spectrum FPS (value 5-30)
    SpectrumFps = 0x08,
    /// Spectrum zoom level (value: zoom × 10 as u16, e.g. 10=1x, 320=32x, 10240=1024x)
    SpectrumZoom = 0x09,
    /// Spectrum pan position (value: (pan + 0.5) × 10000 as u16, 5000=center)
    SpectrumPan = 0x0A,
    /// Filter low cut (value: Hz offset from VFO as i16 cast to u16)
    FilterLow = 0x0B,
    /// Filter high cut (value: Hz offset from VFO as i16 cast to u16)
    FilterHigh = 0x0C,
    /// Thetis starting indicator (server→client, value 0/1)
    ThetisStarting = 0x0D,
    /// RX2 enable on/off (value 0/1)
    Rx2Enable = 0x0E,
    /// RX2 AF gain (value 0-100, CAT: ZZLB)
    Rx2AfGain = 0x0F,
    /// RX2 spectrum zoom (same encoding as SpectrumZoom)
    Rx2SpectrumZoom = 0x10,
    /// RX2 spectrum pan (same encoding as SpectrumPan)
    Rx2SpectrumPan = 0x11,
    /// RX2 filter low cut (same encoding as FilterLow)
    Rx2FilterLow = 0x12,
    /// RX2 filter high cut (same encoding as FilterHigh)
    Rx2FilterHigh = 0x13,
    /// VFO sync on/off (value 0/1: VFO-B follows VFO-A frequency)
    VfoSync = 0x14,
    /// RX2 spectrum enable (value 0=off, 1=on)
    Rx2SpectrumEnable = 0x15,
    /// RX2 spectrum FPS (value 5-30)
    Rx2SpectrumFps = 0x16,
    /// RX2 noise reduction (same encoding as NoiseReduction)
    Rx2NoiseReduction = 0x17,
    /// RX2 auto notch filter (value 0/1)
    Rx2AutoNotchFilter = 0x18,
    /// VFO A⇔B swap (write-only trigger, value ignored; maps to ZZVS2)
    VfoSwap = 0x19,
    /// Spectrum max bins per packet (value: max bins as u16, 0=server default)
    SpectrumMaxBins = 0x1A,
    /// RX2 spectrum max bins per packet (same encoding as SpectrumMaxBins)
    Rx2SpectrumMaxBins = 0x1B,
    /// FFT size for spectrum processing (value: size in K, e.g. 32=32768, 65=65536, 131=131072, 262=262144)
    SpectrumFftSize = 0x1C,
    /// RX2 FFT size for spectrum processing (same encoding as SpectrumFftSize)
    Rx2SpectrumFftSize = 0x3F,
    /// Spectrum bin depth: 8 = u8 bins (1 byte), 16 = u16 bins (2 bytes). Default 8.
    SpectrumBinDepth = 0x1D,
    /// TX Monitor on/off (value 0/1, CAT: ZZMO, TCI: MON_ENABLE)
    MonitorOn = 0x1E,
    /// Thetis TUNE on/off (value 0/1, CAT: ZZTU)
    ThetisTune = 0x1F,
    /// Yaesu stream enable (value 0/1: client requests Yaesu audio+state on/off)
    YaesuEnable = 0x20,
    /// Yaesu PTT (value 0/1: TX0/TX1 via Yaesu CAT)
    YaesuPtt = 0x21,
    /// Yaesu frequency set (uses FrequencyPacket, not ControlPacket)
    YaesuFreq = 0x22,
    /// Yaesu mic gain (value: gain × 10, e.g. 200 = 20.0x)
    YaesuMicGain = 0x23,
    /// Yaesu operating mode (value: internal mode number)
    YaesuMode = 0x24,
    /// Yaesu read all memories (value: ignored)
    YaesuReadMemories = 0x25,
    /// Yaesu recall memory channel (value: channel number 1-99)
    YaesuRecallMemory = 0x26,
    /// Yaesu write all memories to radio (value: ignored)
    YaesuWriteMemories = 0x27,
    /// Yaesu select VFO (value: 0=VFO A, 1=VFO B, 2=swap)
    YaesuSelectVfo = 0x28,
    /// Yaesu squelch (value: 0-255)
    YaesuSquelch = 0x29,
    /// Yaesu RF gain (value: 0-255)
    YaesuRfGain = 0x2A,
    /// Yaesu mic gain (value: 0-100) — radio's own mic gain, not ThetisLink TX gain
    YaesuRadioMicGain = 0x2B,
    /// Yaesu RF power (value: 0-100)
    YaesuRfPower = 0x2C,
    /// Yaesu raw CAT button (value: button ID, see YAESU_BUTTONS)
    YaesuButton = 0x2D,
    /// Yaesu read all EX menu settings (value: ignored)
    YaesuReadMenus = 0x2E,
    /// Yaesu set EX menu item (value: menu number, P2 in separate data packet)
    YaesuSetMenu = 0x2F,
    /// AGC mode (value: 0=off, 1=long, 2=slow, 3=med, 4=fast, 5=custom; TCI: agc_mode)
    AgcMode = 0x30,
    /// AGC gain (value: 0-120; TCI: agc_gain)
    AgcGain = 0x31,
    /// RIT enable (value: 0/1; TCI: rit_enable)
    RitEnable = 0x32,
    /// RIT offset in Hz (value: i16 cast to u16; TCI: rit_offset)
    RitOffset = 0x33,
    /// XIT enable (value: 0/1; TCI: xit_enable)
    XitEnable = 0x34,
    /// XIT offset in Hz (value: i16 cast to u16; TCI: xit_offset)
    XitOffset = 0x35,
    /// Squelch enable (value: 0/1; TCI: sql_enable)
    SqlEnable = 0x36,
    /// Squelch level (value: 0-160; TCI: sql_level)
    SqlLevel = 0x37,
    /// Noise Blanker enable (value: 0/1; TCI: rx_nb_enable)
    NoiseBlanker = 0x38,
    /// CW keyer speed (value: 1-60 WPM; TCI: cw_keyer_speed)
    CwKeyerSpeed = 0x39,
    /// VFO lock (value: 0/1; TCI: vfo_lock)
    VfoLock = 0x3A,
    /// Binaural enable (value: 0/1; TCI: rx_bin_enable)
    Binaural = 0x3B,
    /// Audio Peak Filter enable (value: 0/1; TCI: rx_apf_enable)
    ApfEnable = 0x3C,

    /// RX2 AGC mode (same encoding as AgcMode)
    Rx2AgcMode = 0x50,
    /// RX2 AGC gain (same encoding as AgcGain)
    Rx2AgcGain = 0x51,
    /// RX2 Squelch enable (same encoding as SqlEnable)
    Rx2SqlEnable = 0x52,
    /// RX2 Squelch level (same encoding as SqlLevel)
    Rx2SqlLevel = 0x53,
    /// RX2 Noise Blanker enable (same encoding as NoiseBlanker)
    Rx2NoiseBlanker = 0x54,
    /// RX2 Binaural enable (same encoding as Binaural)
    Rx2Binaural = 0x55,
    /// RX2 Audio Peak Filter enable (same encoding as ApfEnable)
    Rx2ApfEnable = 0x56,
    /// RX2 VFO lock (value: 0/1; TCI: vfo_lock:0,1)
    Rx2VfoLock = 0x57,
    /// Tune drive level (value: 0-100; TCI: tune_drive)
    TuneDrive = 0x58,
    /// Monitor volume in dB (value: i8 as u16, typically -40..0; TCI: mon_volume)
    MonitorVolume = 0x59,

    /// Trigger server-side diversity auto-null (value: 1=start)
    DiversityAutoNull = 0x4A,
    /// AGC Auto mode RX1 (value: 0=off, 1=on)
    AgcAutoRx1 = 0x48,
    /// AGC Auto mode RX2 (value: 0=off, 1=on)
    AgcAutoRx2 = 0x49,

    /// DDC sample rate RX1 (value: rate in kHz, e.g. 384 = 384000 Hz).
    /// Bron: stock TCI `iq_samplerate` (primary) of `if_limits` (fallback) — zie
    /// `PATCH-tl2-server-if-limits` (alpha-3). Beide RX1 en RX2 krijgen dezelfde
    /// waarde in stock-mode (TCI exposes één globale rate). Per-RX divergence
    /// komt terug via TL2-x fork extensions in Phase 3.
    DdcSampleRateRx1 = 0x3D,
    /// DDC sample rate RX2 (zelfde encoding als DdcSampleRateRx1).
    DdcSampleRateRx2 = 0x3E,

    /// Diversity enable (value: 0=off, 1=on)
    DiversityEnable = 0x40,
    /// Diversity reference source (value: 0=RX2, 1=RX1)
    DiversityRef = 0x41,
    /// Diversity RX source (value: 0=RX1+RX2, 1=RX1, 2=RX2)
    DiversitySource = 0x42,
    /// Diversity RX1 gain (value: gain × 1000, e.g. 2500 = 2.500)
    DiversityGainRx1 = 0x43,
    /// Diversity RX2 gain (value: gain × 1000, e.g. 2500 = 2.500)
    DiversityGainRx2 = 0x44,
    /// Diversity phase (value: phase × 100 + 18000, e.g. 18000=0°, 0=-180°, 36000=+180°)
    DiversityPhase = 0x45,
    /// Read diversity state from Thetis (value: ignored)
    DiversityRead = 0x46,
    /// Diversity GainMulti — gates per-RX gain max (value: multi × 100, range 100..1000 = 1.00..10.00).
    /// TL2-1 fork-only `diversity_gain_multi_ex` command.
    DiversityGainMulti = 0x47,

    /// Global mute (value: 0/1; TCI: mute)
    Mute = 0x5A,
    /// RX mute (value: 0/1; TCI: rx_mute:0)
    RxMute = 0x5B,
    /// Manual Notch Filter enable (value: 0/1; TCI: rx_nf_enable:0)
    ManualNotchFilter = 0x5C,
    /// RX Balance L/R pan (value: i8 -40..+40 as i16 cast to u16; TCI: rx_balance:0,0)
    RxBalance = 0x5D,

    /// CW keyer key down/up (value: bit 0 = pressed, bits 1-15 = duration_ms; 0 = no duration)
    CwKey = 0x5E,
    /// CW macro stop (value: ignored; TCI: cw_macros_stop)
    CwMacroStop = 0x5F,

    /// RX2 Manual Notch Filter enable (value: 0/1; TCI: rx_nf_enable:1)
    Rx2ManualNotchFilter = 0x60,
    /// Thetis SWR (value: SWR × 100, e.g. 150 = 1.50:1; server → client broadcast during TX)
    ThetisSwr = 0x61,
    /// Audio routing mode (0=Mono RX1→L+R, 1=Binaural RX1L+RX1R, 2=Split RX1→L RX2→R)
    AudioMode = 0x62,

    /// Per-client setup-vink "Allow zoom below 2× (waterfall smear during tune)".
    /// Value: 0=vink-uit (default, smear-vrij gegarandeerd), 1=vink-aan (zoom 1× toegestaan, smear-trade-off).
    /// TL2-1 server enforces strictest setting over all connected clients (zoom-min 2×
    /// zolang één client vink-uit heeft). Used by `auto_recenter_ex` feature.
    AllowZoomBelow2x = 0x63,

    /// Per-client S-meter source-subscription bitmap. Each bit toggles emission
    /// of one S-meter packet type by the server. Default (no control sent):
    /// `0x22` = RX1 Avg + RX2 Avg, matches pre-multi-source behaviour.
    ///
    /// Bitmap layout (u16):
    ///   bit 0  = RX1 Sig    → PacketType::SmeterSig         (WDSP RXA_S_PK)
    ///   bit 1  = RX1 Avg    → PacketType::Smeter            (WDSP RXA_S_AV)
    ///   bit 2  = RX1 MaxBin → PacketType::SmeterMaxBin      (single peak FFT bin)
    ///   bit 4  = RX2 Sig    → PacketType::SmeterRx2Sig
    ///   bit 5  = RX2 Avg    → PacketType::SmeterRx2         (existing)
    ///   bit 6  = RX2 MaxBin → PacketType::SmeterRx2MaxBin
    /// All other bits reserved.
    SmeterSources = 0x64,
    /// DX-spot stream opt-out (value 0=off, 1=on). Default ON. Wanneer OFF
    /// stuurt de server geen `PacketType::Spot`-frames meer naar deze client
    /// — bandbreedte-besparing op metered links.
    DxSpotsEnabled = 0x65,
}

impl ControlId {
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::try_from(v).ok()
    }
}

// ControlId::from_u8 and PacketType::from_u8 use num_enum TryFromPrimitive derive.
// Manual match blocks removed — the #[repr(u8)] values are the single source of truth.

/// Capability flags for protocol negotiation (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities(pub u32);

impl Capabilities {
    pub const NONE: Self = Self(0);
    /// Support for 16kHz wideband Opus audio
    pub const WIDEBAND_AUDIO: u32 = 1 << 0;
    /// Support for spectrum/waterfall data
    pub const SPECTRUM: u32 = 1 << 1;
    /// Support for RX2/VFO-B dual receiver
    pub const RX2: u32 = 1 << 2;
    /// Server reports runtime state via the `state_flags` field in HeartbeatAck
    /// (PATCH-1). Client MUST check this bit before trusting `state_flags`;
    /// older servers (v2.0.0 release-tag and earlier) leave `state_flags` at
    /// NONE which would otherwise be misread as "TCI down". When this bit is
    /// clear in the negotiated capabilities, the client treats `state_flags`
    /// as unknown / not-authoritative.
    pub const REPORTS_STATE_FLAGS: u32 = 1 << 3;

    pub fn has(self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    pub fn with(self, flag: u32) -> Self {
        Self(self.0 | flag)
    }

    /// Return the intersection of two capability sets (features both sides support)
    pub fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// Flags byte — bit 0 = PTT active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags(pub u8);

impl Flags {
    pub const NONE: Self = Self(0);
    pub const PTT: Self = Self(0x01);

    pub fn ptt(self) -> bool {
        self.0 & 0x01 != 0
    }

    pub fn with_ptt(self, ptt: bool) -> Self {
        if ptt {
            Self(self.0 | 0x01)
        } else {
            Self(self.0 & !0x01)
        }
    }
}

/// Common 4-byte header: magic, version, packet_type, flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub packet_type: PacketType,
    pub flags: Flags,
}

impl Header {
    pub const SIZE: usize = 4;

    pub fn new(packet_type: PacketType, flags: Flags) -> Self {
        Self { packet_type, flags }
    }

    pub fn serialize(&self, buf: &mut [u8]) {
        debug_assert!(buf.len() >= Self::SIZE);
        buf[0] = MAGIC;
        buf[1] = VERSION;
        buf[2] = self.packet_type as u8;
        buf[3] = self.flags.0;
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            bail!("packet too short for header: {} bytes", buf.len());
        }
        if buf[0] != MAGIC {
            bail!("invalid magic byte: 0x{:02X}", buf[0]);
        }
        if buf[1] != VERSION {
            bail!("unsupported version: {}", buf[1]);
        }
        let packet_type = PacketType::from_u8(buf[2])
            .ok_or_else(|| anyhow::anyhow!("unknown packet type: 0x{:02X}", buf[2]))?;
        let flags = Flags(buf[3]);
        Ok(Self { packet_type, flags })
    }
}

/// Audio packet: header(4) + sequence(4) + timestamp(4) + opus_len(2) + opus_data(N)
/// Total: 14 + N bytes
#[derive(Debug, Clone)]
pub struct AudioPacket {
    pub flags: Flags,
    pub sequence: u32,
    pub timestamp: u32,
    pub opus_data: Vec<u8>,
}

impl AudioPacket {
    pub const HEADER_SIZE: usize = Header::SIZE + 4 + 4 + 2; // 14 bytes

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.serialize_as_type(buf, PacketType::Audio);
    }

    pub fn serialize_as_type(&self, buf: &mut Vec<u8>, ptype: PacketType) {
        let start = buf.len();
        buf.resize(start + Self::HEADER_SIZE + self.opus_data.len(), 0);
        let out = &mut buf[start..];

        let header = Header::new(ptype, self.flags);
        header.serialize(out);
        out[4..8].copy_from_slice(&self.sequence.to_be_bytes());
        out[8..12].copy_from_slice(&self.timestamp.to_be_bytes());
        out[12..14].copy_from_slice(&(self.opus_data.len() as u16).to_be_bytes());
        out[14..14 + self.opus_data.len()].copy_from_slice(&self.opus_data);
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Audio && header.packet_type != PacketType::AudioRx2
            && header.packet_type != PacketType::AudioYaesu && header.packet_type != PacketType::AudioBinR {
            bail!("expected Audio packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::HEADER_SIZE {
            bail!(
                "audio packet too short: {} < {}",
                buf.len(),
                Self::HEADER_SIZE
            );
        }

        let sequence = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        let timestamp = u32::from_be_bytes(buf[8..12].try_into().unwrap());
        let opus_len = u16::from_be_bytes(buf[12..14].try_into().unwrap()) as usize;

        if buf.len() < Self::HEADER_SIZE + opus_len {
            bail!(
                "audio packet truncated: {} < {}",
                buf.len(),
                Self::HEADER_SIZE + opus_len
            );
        }

        let opus_data = buf[14..14 + opus_len].to_vec();

        Ok(Self {
            flags: header.flags,
            sequence,
            timestamp,
            opus_data,
        })
    }
}

/// Multi-channel audio packet: bundles 1-4 mono Opus frames in one UDP packet.
/// Perfect sync: all channels share one sequence number and timestamp.
///
/// Format:
///   header(4) + sequence(4) + timestamp(4) + channel_count(1) = 13 bytes
///   Per channel: channel_id(1) + opus_len(2) + opus_data(N)
///
/// Channel IDs:
///   0 = RX1 (or RX1-L when binaural)
///   1 = RX1-R (binaural right; absent when BIN off)
///   2 = RX2
///   3 = Yaesu (reserved for future bundling)
#[derive(Debug, Clone)]
pub struct MultiChannelAudioPacket {
    pub sequence: u32,
    pub timestamp: u32,
    pub channels: Vec<(u8, Vec<u8>)>, // (channel_id, opus_data)
}

impl MultiChannelAudioPacket {
    pub const HEADER_SIZE: usize = Header::SIZE + 4 + 4 + 1; // 13 bytes

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let payload_size: usize = self.channels.iter().map(|(_, d)| 1 + 2 + d.len()).sum();
        let start = buf.len();
        buf.resize(start + Self::HEADER_SIZE + payload_size, 0);
        let out = &mut buf[start..];

        let header = Header::new(PacketType::AudioMultiCh, Flags::NONE);
        header.serialize(out);
        out[4..8].copy_from_slice(&self.sequence.to_be_bytes());
        out[8..12].copy_from_slice(&self.timestamp.to_be_bytes());
        out[12] = self.channels.len() as u8;

        let mut pos = Self::HEADER_SIZE;
        for (ch_id, opus_data) in &self.channels {
            out[pos] = *ch_id;
            out[pos + 1..pos + 3].copy_from_slice(&(opus_data.len() as u16).to_be_bytes());
            out[pos + 3..pos + 3 + opus_data.len()].copy_from_slice(opus_data);
            pos += 3 + opus_data.len();
        }
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::AudioMultiCh {
            bail!("expected AudioMultiCh packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::HEADER_SIZE {
            bail!("multi-ch audio packet too short: {} < {}", buf.len(), Self::HEADER_SIZE);
        }

        let sequence = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        let timestamp = u32::from_be_bytes(buf[8..12].try_into().unwrap());
        let channel_count = buf[12] as usize;

        let mut channels = Vec::with_capacity(channel_count);
        let mut pos = Self::HEADER_SIZE;
        for _ in 0..channel_count {
            if pos + 3 > buf.len() { break; }
            let ch_id = buf[pos];
            let opus_len = u16::from_be_bytes(buf[pos + 1..pos + 3].try_into().unwrap()) as usize;
            if pos + 3 + opus_len > buf.len() { break; }
            let opus_data = buf[pos + 3..pos + 3 + opus_len].to_vec();
            channels.push((ch_id, opus_data));
            pos += 3 + opus_len;
        }

        Ok(Self { sequence, timestamp, channels })
    }
}

/// Heartbeat packet: header(4) + sequence(4) + local_time(4) + rtt(u16) + loss(u8) + jitter(u8) + capabilities(4)
/// Total: 20 bytes (backward compatible: 16 bytes without capabilities)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heartbeat {
    pub flags: Flags,
    pub sequence: u32,
    /// Local timestamp in milliseconds (wrapping)
    pub local_time: u32,
    /// Last measured RTT in milliseconds
    pub rtt_ms: u16,
    /// Packet loss percentage (0-100)
    pub loss_percent: u8,
    /// Jitter in milliseconds
    pub jitter_ms: u8,
    /// Client capabilities (0 if not present)
    pub capabilities: Capabilities,
}

impl Heartbeat {
    /// Minimum size for backward compatibility (without capabilities)
    pub const MIN_SIZE: usize = 16;
    /// Full size including capabilities
    pub const SIZE: usize = 20;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::Heartbeat, self.flags);
        header.serialize(buf);
        buf[4..8].copy_from_slice(&self.sequence.to_be_bytes());
        buf[8..12].copy_from_slice(&self.local_time.to_be_bytes());
        buf[12..14].copy_from_slice(&self.rtt_ms.to_be_bytes());
        buf[14] = self.loss_percent;
        buf[15] = self.jitter_ms;
        buf[16..20].copy_from_slice(&self.capabilities.0.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Heartbeat {
            bail!("expected Heartbeat packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::MIN_SIZE {
            bail!("heartbeat too short: {} < {}", buf.len(), Self::MIN_SIZE);
        }

        let capabilities = if buf.len() >= Self::SIZE {
            Capabilities(u32::from_be_bytes(buf[16..20].try_into().unwrap()))
        } else {
            Capabilities::NONE
        };

        Ok(Self {
            flags: header.flags,
            sequence: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            local_time: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            rtt_ms: u16::from_be_bytes(buf[12..14].try_into().unwrap()),
            loss_percent: buf[14],
            jitter_ms: buf[15],
            capabilities,
        })
    }
}

/// Server-state flags broadcast in HeartbeatAck. Independent from
/// `Capabilities` (which describes feature negotiation, static); this
/// describes runtime state (dynamic).
///
/// Added in v2.0.0-post (PATCH-1 client-connect-error-feedback) for
/// the client to detect "TCI not reachable on server PC" without
/// inferring from audio absence (false-positives on quiet bands).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerStateFlags(pub u32);

impl ServerStateFlags {
    pub const NONE: Self = Self(0);
    /// Server's TCI WebSocket to Thetis is currently up.
    /// When clear: TCI is down, retrying, or unreachable.
    pub const TCI_CONNECTED: u32 = 1 << 0;
    /// Thetis.exe process is currently running on the server PC.
    /// Used by the client to give a smarter TciUnreachable hint:
    /// - flag set + TCI_CONNECTED clear → "Thetis runs, check TCI server settings"
    /// - flag clear → "Thetis is not running, press Start on the Thetis tab"
    /// Only trustworthy when capabilities advertises REPORTS_STATE_FLAGS.
    pub const THETIS_RUNNING: u32 = 1 << 1;
    /// Server is in the middle of launching Thetis (orange "Start" button).
    /// Set between user pressing Start and TCI connecting (or the 60s
    /// launch timeout firing). Client should NOT show "TCI unreachable"
    /// while this is set — it's a normal transient launch phase.
    pub const THETIS_STARTING: u32 = 1 << 2;

    pub fn has(self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    pub fn with(self, flag: u32) -> Self {
        Self(self.0 | flag)
    }
}

/// HeartbeatAck: header(4) + echo_seq(4) + echo_time(4) + capabilities(4) + state_flags(4) = 20 bytes
/// Backward compatible: 12 bytes (just header+echo) or 16 bytes (+capabilities) accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub flags: Flags,
    pub echo_sequence: u32,
    pub echo_time: u32,
    /// Server capabilities (negotiated: intersection of client + server caps)
    pub capabilities: Capabilities,
    /// Server runtime-state flags (e.g. TCI_CONNECTED). Forward-compat:
    /// older servers don't send this; client treats absent as `NONE`.
    pub state_flags: ServerStateFlags,
}

impl HeartbeatAck {
    /// Minimum size for backward compatibility (without capabilities or state_flags)
    pub const MIN_SIZE: usize = 12;
    /// Size including capabilities (v0.x .. v2.0.0 release)
    pub const SIZE_WITH_CAPS: usize = 16;
    /// Full size including state_flags (v2.0.0-post PATCH-1)
    pub const SIZE: usize = 20;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::HeartbeatAck, self.flags);
        header.serialize(buf);
        buf[4..8].copy_from_slice(&self.echo_sequence.to_be_bytes());
        buf[8..12].copy_from_slice(&self.echo_time.to_be_bytes());
        buf[12..16].copy_from_slice(&self.capabilities.0.to_be_bytes());
        buf[16..20].copy_from_slice(&self.state_flags.0.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::HeartbeatAck {
            bail!("expected HeartbeatAck, got {:?}", header.packet_type);
        }
        if buf.len() < Self::MIN_SIZE {
            bail!("heartbeat ack too short: {} < {}", buf.len(), Self::MIN_SIZE);
        }

        let capabilities = if buf.len() >= Self::SIZE_WITH_CAPS {
            Capabilities(u32::from_be_bytes(buf[12..16].try_into().unwrap()))
        } else {
            Capabilities::NONE
        };

        let state_flags = if buf.len() >= Self::SIZE {
            ServerStateFlags(u32::from_be_bytes(buf[16..20].try_into().unwrap()))
        } else {
            ServerStateFlags::NONE
        };

        Ok(Self {
            flags: header.flags,
            echo_sequence: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            echo_time: u32::from_be_bytes(buf[8..12].try_into().unwrap()),
            capabilities,
            state_flags,
        })
    }
}

/// Control packet: header(4) + control_id(1) + value(2) = 7 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlPacket {
    pub control_id: ControlId,
    pub value: u16,
}

impl ControlPacket {
    pub const SIZE: usize = 7;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::Control, Flags::NONE);
        header.serialize(buf);
        buf[4] = self.control_id as u8;
        buf[5..7].copy_from_slice(&self.value.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Control {
            bail!("expected Control packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::SIZE {
            bail!("control packet too short: {} < {}", buf.len(), Self::SIZE);
        }
        let control_id = ControlId::from_u8(buf[4])
            .ok_or_else(|| anyhow::anyhow!("unknown control id: 0x{:02X}", buf[4]))?;
        let value = u16::from_be_bytes(buf[5..7].try_into().unwrap());
        Ok(Self { control_id, value })
    }
}

/// Disconnect packet: just a header (4 bytes)
pub struct DisconnectPacket;

impl DisconnectPacket {
    pub const SIZE: usize = Header::SIZE;

    pub fn serialize(buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::Disconnect, Flags::NONE);
        header.serialize(buf);
    }
}

/// PTT denied packet: just a header (4 bytes)
/// Sent by server when a client's PTT request is rejected because another client holds TX.
pub struct PttDeniedPacket;

impl PttDeniedPacket {
    pub const SIZE: usize = Header::SIZE;

    pub fn serialize(buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::PttDenied, Flags::NONE);
        header.serialize(buf);
    }
}

/// Frequency packet: header(4) + frequency_hz(8) = 12 bytes
/// Bidirectional: server→client (readback) and client→server (set)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrequencyPacket {
    pub frequency_hz: u64,
}

impl FrequencyPacket {
    pub const SIZE: usize = 12;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        self.serialize_as_type(buf, PacketType::Frequency);
    }

    pub fn serialize_as_type(&self, buf: &mut [u8; Self::SIZE], ptype: PacketType) {
        let header = Header::new(ptype, Flags::NONE);
        header.serialize(buf);
        buf[4..12].copy_from_slice(&self.frequency_hz.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Frequency && header.packet_type != PacketType::FrequencyRx2 && header.packet_type != PacketType::FrequencyYaesu {
            bail!("expected Frequency/FrequencyRx2 packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::SIZE {
            bail!("frequency packet too short: {} < {}", buf.len(), Self::SIZE);
        }
        let frequency_hz = u64::from_be_bytes(buf[4..12].try_into().unwrap());
        Ok(Self { frequency_hz })
    }
}

/// Mode packet: header(4) + mode(1) = 5 bytes
/// Bidirectional: server→client (readback) and client→server (set)
/// Mode values: 00=LSB, 01=USB, 05=FM, 06=AM (Thetis ZZMD values)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModePacket {
    pub mode: u8,
}

impl ModePacket {
    pub const SIZE: usize = 5;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        self.serialize_as_type(buf, PacketType::Mode);
    }

    pub fn serialize_as_type(&self, buf: &mut [u8; Self::SIZE], ptype: PacketType) {
        let header = Header::new(ptype, Flags::NONE);
        header.serialize(buf);
        buf[4] = self.mode;
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Mode && header.packet_type != PacketType::ModeRx2 {
            bail!("expected Mode/ModeRx2 packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::SIZE {
            bail!("mode packet too short: {} < {}", buf.len(), Self::SIZE);
        }
        Ok(Self { mode: buf[4] })
    }
}

/// S-meter packet: header(4) + level(2) = 6 bytes
/// Server→client only. Level is signed deci-units:
///  - `Flags::PTT` clear (RX): dBm × 10 (typically negative, e.g. -730 = -73 dBm = S9).
///  - `Flags::PTT` set (TX): watts × 10 (positive, e.g. 1000 = 100.0 W FWD).
/// The client owns all display-scale math; the wire format carries the raw
/// physical quantity so changes to the visual scale never break wire compat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmeterPacket {
    pub level: i16,
    pub flags: Flags,
}

impl SmeterPacket {
    pub const SIZE: usize = 6;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        self.serialize_as_type(buf, PacketType::Smeter);
    }

    pub fn serialize_as_type(&self, buf: &mut [u8; Self::SIZE], ptype: PacketType) {
        let header = Header::new(ptype, self.flags);
        header.serialize(buf);
        buf[4..6].copy_from_slice(&self.level.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if !matches!(
            header.packet_type,
            PacketType::Smeter
                | PacketType::SmeterRx2
                | PacketType::SmeterSig
                | PacketType::SmeterMaxBin
                | PacketType::SmeterRx2Sig
                | PacketType::SmeterRx2MaxBin
        ) {
            bail!("expected Smeter packet, got {:?}", header.packet_type);
        }
        if buf.len() < Self::SIZE {
            bail!("smeter packet too short: {} < {}", buf.len(), Self::SIZE);
        }
        let level = i16::from_be_bytes(buf[4..6].try_into().unwrap());
        Ok(Self { level, flags: header.flags })
    }
}

/// Spectrum packet: header(4) + sequence(2) + num_bins(2) + center_freq_hz(4) + span_hz(4) + ref_level(1) + db_per_unit(1) + bins(N×B)
/// db_per_unit encodes bin byte width: 1 = u8 bins (0-255), 2 = u16 bins (0-65535).
/// Bins internally stored as Vec<u16>; u8 packets are upscaled ×257 on receive.
#[derive(Debug, Clone)]
pub struct SpectrumPacket {
    pub sequence: u16,
    pub num_bins: u16,
    pub center_freq_hz: u32,
    pub span_hz: u32,
    pub ref_level: i8,
    pub db_per_unit: u8,  // 1 = u8 bins on wire, 2 = u16 bins on wire
    pub bins: Vec<u16>,
}

impl SpectrumPacket {
    pub const HEADER_SIZE: usize = Header::SIZE + 2 + 2 + 4 + 4 + 1 + 1; // 18 bytes

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        self.serialize_as_type(buf, PacketType::Spectrum);
    }

    pub fn serialize_as_type(&self, buf: &mut Vec<u8>, ptype: PacketType) {
        let start = buf.len();
        let bytes_per_bin = if self.db_per_unit == 2 { 2 } else { 1 };
        let bin_bytes = self.bins.len() * bytes_per_bin;
        buf.resize(start + Self::HEADER_SIZE + bin_bytes, 0);
        let out = &mut buf[start..];

        let header = Header::new(ptype, Flags::NONE);
        header.serialize(out);
        out[4..6].copy_from_slice(&self.sequence.to_be_bytes());
        out[6..8].copy_from_slice(&self.num_bins.to_be_bytes());
        out[8..12].copy_from_slice(&self.center_freq_hz.to_be_bytes());
        out[12..16].copy_from_slice(&self.span_hz.to_be_bytes());
        out[16] = self.ref_level as u8;
        out[17] = self.db_per_unit;
        if self.db_per_unit == 2 {
            // u16 bins: 2 bytes each, big-endian
            for (i, &val) in self.bins.iter().enumerate() {
                let offset = Self::HEADER_SIZE + i * 2;
                out[offset..offset + 2].copy_from_slice(&val.to_be_bytes());
            }
        } else {
            // u8 bins: 1 byte each (downscale from u16 → u8)
            for (i, &val) in self.bins.iter().enumerate() {
                out[Self::HEADER_SIZE + i] = (val >> 8) as u8;
            }
        }
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if !matches!(header.packet_type, PacketType::Spectrum | PacketType::FullSpectrum | PacketType::SpectrumRx2 | PacketType::FullSpectrumRx2) {
            bail!("expected Spectrum packet variant, got {:?}", header.packet_type);
        }
        if buf.len() < Self::HEADER_SIZE {
            bail!(
                "spectrum packet too short: {} < {}",
                buf.len(),
                Self::HEADER_SIZE
            );
        }

        let sequence = u16::from_be_bytes(buf[4..6].try_into().unwrap());
        let num_bins = u16::from_be_bytes(buf[6..8].try_into().unwrap());
        let center_freq_hz = u32::from_be_bytes(buf[8..12].try_into().unwrap());
        let span_hz = u32::from_be_bytes(buf[12..16].try_into().unwrap());
        let ref_level = buf[16] as i8;
        let db_per_unit = buf[17];

        let bytes_per_bin = if db_per_unit == 2 { 2 } else { 1 };
        let expected_len = Self::HEADER_SIZE + num_bins as usize * bytes_per_bin;
        if buf.len() < expected_len {
            bail!(
                "spectrum packet truncated: {} < {}",
                buf.len(),
                expected_len
            );
        }

        let bins: Vec<u16> = if db_per_unit == 2 {
            // u16 bins: 2 bytes each
            (0..num_bins as usize)
                .map(|i| {
                    let offset = Self::HEADER_SIZE + i * 2;
                    u16::from_be_bytes(buf[offset..offset + 2].try_into().unwrap())
                })
                .collect()
        } else {
            // u8 bins: upscale to u16 (×257 maps 0-255 → 0-65535)
            (0..num_bins as usize)
                .map(|i| {
                    let v = buf[Self::HEADER_SIZE + i] as u16;
                    v | (v << 8)  // equivalent to v * 257
                })
                .collect()
        };

        Ok(Self {
            sequence,
            num_bins,
            center_freq_hz,
            span_hz,
            ref_level,
            db_per_unit,
            bins,
        })
    }
}

/// DX Cluster spot packet: header(4) + callsign_len(1) + callsign + freq_hz(8) + mode_len(1) + mode + spotter_len(1) + spotter + comment_len(1) + comment + age_seconds(2)
#[derive(Debug, Clone)]
pub struct SpotPacket {
    pub callsign: String,
    pub frequency_hz: u64,
    pub mode: String,
    pub spotter: String,
    pub comment: String,
    pub age_seconds: u16,
    /// Total spot lifetime in seconds (from config, e.g. 600 = 10 min)
    pub expiry_seconds: u16,
}

impl SpotPacket {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let callsign_bytes = self.callsign.as_bytes();
        let mode_bytes = self.mode.as_bytes();
        let spotter_bytes = self.spotter.as_bytes();
        let comment_bytes = self.comment.as_bytes();
        let total = Header::SIZE + 1 + callsign_bytes.len() + 8 + 1 + mode_bytes.len()
            + 1 + spotter_bytes.len() + 1 + comment_bytes.len() + 2 + 2;

        let start = buf.len();
        buf.resize(start + total, 0);
        let out = &mut buf[start..];

        let header = Header::new(PacketType::Spot, Flags::NONE);
        header.serialize(out);
        let mut pos = Header::SIZE;

        out[pos] = callsign_bytes.len() as u8;
        pos += 1;
        out[pos..pos + callsign_bytes.len()].copy_from_slice(callsign_bytes);
        pos += callsign_bytes.len();

        out[pos..pos + 8].copy_from_slice(&self.frequency_hz.to_be_bytes());
        pos += 8;

        out[pos] = mode_bytes.len() as u8;
        pos += 1;
        out[pos..pos + mode_bytes.len()].copy_from_slice(mode_bytes);
        pos += mode_bytes.len();

        out[pos] = spotter_bytes.len() as u8;
        pos += 1;
        out[pos..pos + spotter_bytes.len()].copy_from_slice(spotter_bytes);
        pos += spotter_bytes.len();

        out[pos] = comment_bytes.len() as u8;
        pos += 1;
        out[pos..pos + comment_bytes.len()].copy_from_slice(comment_bytes);
        pos += comment_bytes.len();

        out[pos..pos + 2].copy_from_slice(&self.age_seconds.to_be_bytes());
        pos += 2;
        out[pos..pos + 2].copy_from_slice(&self.expiry_seconds.to_be_bytes());
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::Spot {
            bail!("expected Spot packet, got {:?}", header.packet_type);
        }
        let mut pos = Header::SIZE;

        if pos >= buf.len() { bail!("spot packet truncated at callsign_len"); }
        let callsign_len = buf[pos] as usize;
        pos += 1;
        if pos + callsign_len > buf.len() { bail!("spot packet truncated at callsign"); }
        let callsign = String::from_utf8_lossy(&buf[pos..pos + callsign_len]).to_string();
        pos += callsign_len;

        if pos + 8 > buf.len() { bail!("spot packet truncated at freq"); }
        let frequency_hz = u64::from_be_bytes(buf[pos..pos + 8].try_into().unwrap());
        pos += 8;

        if pos >= buf.len() { bail!("spot packet truncated at mode_len"); }
        let mode_len = buf[pos] as usize;
        pos += 1;
        if pos + mode_len > buf.len() { bail!("spot packet truncated at mode"); }
        let mode = String::from_utf8_lossy(&buf[pos..pos + mode_len]).to_string();
        pos += mode_len;

        if pos >= buf.len() { bail!("spot packet truncated at spotter_len"); }
        let spotter_len = buf[pos] as usize;
        pos += 1;
        if pos + spotter_len > buf.len() { bail!("spot packet truncated at spotter"); }
        let spotter = String::from_utf8_lossy(&buf[pos..pos + spotter_len]).to_string();
        pos += spotter_len;

        if pos >= buf.len() { bail!("spot packet truncated at comment_len"); }
        let comment_len = buf[pos] as usize;
        pos += 1;
        if pos + comment_len > buf.len() { bail!("spot packet truncated at comment"); }
        let comment = String::from_utf8_lossy(&buf[pos..pos + comment_len]).to_string();
        pos += comment_len;

        if pos + 2 > buf.len() { bail!("spot packet truncated at age"); }
        let age_seconds = u16::from_be_bytes(buf[pos..pos + 2].try_into().unwrap());
        pos += 2;

        // expiry_seconds: optional for backward compat (default 600 = 10 min)
        let expiry_seconds = if pos + 2 <= buf.len() {
            u16::from_be_bytes(buf[pos..pos + 2].try_into().unwrap())
        } else {
            600
        };

        Ok(Self { callsign, frequency_hz, mode, spotter, comment, age_seconds, expiry_seconds })
    }
}

/// Device type identifiers for external equipment
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum DeviceType {
    Amplitec6x2 = 0x01,
    Tuner = 0x02,
    SpeExpert = 0x03,
    Rf2k = 0x04,
    UltraBeam = 0x05,
    Rotor = 0x06,
    RemoteServer = 0x07,
}

impl DeviceType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Self::try_from(v).ok()
    }
}

/// Remote server command IDs (client → server via EquipmentCommand)
pub const CMD_SERVER_REBOOT: u8 = 0x01;
pub const CMD_SERVER_SHUTDOWN: u8 = 0x02;

/// Amplitec 6/2 command IDs (client → server via EquipmentCommand).
/// Switch-positie schakelen heeft geen const — daarvoor zijn
/// `EquipmentCommandPacket::CMD_SET_SWITCH_A/_B` (pre-existing).
/// Voor de power-cap tabel: 18 bytes data
/// (6 × { u16 max_w BE, u8 tx_blocked }).
pub const CMD_AMPLITEC_SET_POWER_TABLE: u8 = 0x10;

/// Tuner command IDs (client → server via EquipmentCommand)
pub const CMD_TUNE_START: u8 = 0x01;
pub const CMD_TUNE_ABORT: u8 = 0x02;

/// SPE Expert command IDs (client → server via EquipmentCommand)
pub const CMD_SPE_OPERATE: u8 = 0x01;
pub const CMD_SPE_TUNE: u8 = 0x02;
pub const CMD_SPE_ANTENNA: u8 = 0x03;
pub const CMD_SPE_INPUT: u8 = 0x04;
pub const CMD_SPE_POWER: u8 = 0x05;
pub const CMD_SPE_BAND_UP: u8 = 0x06;
pub const CMD_SPE_BAND_DOWN: u8 = 0x07;
pub const CMD_SPE_OFF: u8 = 0x08;
pub const CMD_SPE_POWER_ON: u8 = 0x09;
pub const CMD_SPE_DRIVE_DOWN: u8 = 0x0A;
pub const CMD_SPE_DRIVE_UP: u8 = 0x0B;

/// RF2K-S command IDs (client → server via EquipmentCommand)
pub const CMD_RF2K_OPERATE: u8 = 0x01;
pub const CMD_RF2K_TUNE: u8 = 0x02;
pub const CMD_RF2K_ANT1: u8 = 0x03;
pub const CMD_RF2K_ANT2: u8 = 0x04;
pub const CMD_RF2K_ANT3: u8 = 0x05;
pub const CMD_RF2K_ANT4: u8 = 0x06;
pub const CMD_RF2K_ANT_EXT: u8 = 0x07;
pub const CMD_RF2K_ERROR_RESET: u8 = 0x08;
pub const CMD_RF2K_CLOSE: u8 = 0x09;
// Tuner controls (Fase B)
pub const CMD_RF2K_TUNER_MODE: u8 = 0x10;
pub const CMD_RF2K_TUNER_BYPASS: u8 = 0x11;
pub const CMD_RF2K_TUNER_RESET: u8 = 0x12;
pub const CMD_RF2K_TUNER_STORE: u8 = 0x13;
pub const CMD_RF2K_TUNER_L_UP: u8 = 0x14;
pub const CMD_RF2K_TUNER_L_DOWN: u8 = 0x15;
pub const CMD_RF2K_TUNER_C_UP: u8 = 0x16;
pub const CMD_RF2K_TUNER_C_DOWN: u8 = 0x17;
pub const CMD_RF2K_TUNER_K: u8 = 0x18;
// Drive controls (Fase C)
pub const CMD_RF2K_DRIVE_UP: u8 = 0x20;
pub const CMD_RF2K_DRIVE_DOWN: u8 = 0x21;
// Debug controls (Fase D)
pub const CMD_RF2K_SET_HIGH_POWER: u8 = 0x30;   // data[0]: 0/1
pub const CMD_RF2K_SET_TUNER_6M: u8 = 0x31;     // data[0]: 0/1
pub const CMD_RF2K_SET_BAND_GAP: u8 = 0x32;     // data[0]: 0/1
pub const CMD_RF2K_FRQ_DELAY_UP: u8 = 0x33;
pub const CMD_RF2K_FRQ_DELAY_DOWN: u8 = 0x34;
pub const CMD_RF2K_AUTOTUNE_THRESH_UP: u8 = 0x35;
pub const CMD_RF2K_AUTOTUNE_THRESH_DOWN: u8 = 0x36;
pub const CMD_RF2K_DAC_ALC_UP: u8 = 0x37;
pub const CMD_RF2K_DAC_ALC_DOWN: u8 = 0x38;
pub const CMD_RF2K_ZERO_FRAM: u8 = 0x39;
// Drive config (Fase D)
pub const CMD_RF2K_SET_DRIVE_SSB: u8 = 0x40;    // data[0]=band, data[1]=watts
pub const CMD_RF2K_SET_DRIVE_AM: u8 = 0x41;
pub const CMD_RF2K_SET_DRIVE_CONT: u8 = 0x42;

/// UltraBeam RCU-06 command IDs (client → server via EquipmentCommand)
pub const CMD_UB_RETRACT: u8 = 0x01;
pub const CMD_UB_SET_FREQ: u8 = 0x02;  // data[0..1]=khz_le, data[2]=direction
pub const CMD_UB_READ_ELEMENTS: u8 = 0x03;
pub const CMD_UB_MODIFY_ELEMENT: u8 = 0x04;  // data[0]=index, data[1..2]=length_mm_le

/// Rotor command IDs (client → server via EquipmentCommand)
pub const CMD_ROTOR_GOTO: u8 = 0x01;    // data[0..1] = angle_x10 LE (0-3600)
pub const CMD_ROTOR_STOP: u8 = 0x02;
pub const CMD_ROTOR_CW: u8 = 0x03;      // handmatig rechtsom
pub const CMD_ROTOR_CCW: u8 = 0x04;     // handmatig linksom

/// Equipment status flags
const EQUIPMENT_FLAG_HAS_LABELS: u8 = 0x01;

/// Equipment status packet: header(4) + device_type(1) + flags(1) + data(N)
/// For Amplitec6x2: data = switch_a(1) + switch_b(1) + connected(1) [+ labels_len(2) + labels_utf8(N) if has_labels]
#[derive(Debug, Clone)]
pub struct EquipmentStatusPacket {
    pub device_type: DeviceType,
    pub switch_a: u8,
    pub switch_b: u8,
    pub connected: bool,
    pub labels: Option<String>,
}

impl EquipmentStatusPacket {
    pub const MIN_SIZE: usize = Header::SIZE + 1 + 1 + 1 + 1 + 1; // 9 bytes

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let has_labels = self.labels.is_some();
        let flags = if has_labels { EQUIPMENT_FLAG_HAS_LABELS } else { 0 };
        let labels_bytes = self.labels.as_deref().unwrap_or("").as_bytes();
        let total = Self::MIN_SIZE + if has_labels { 2 + labels_bytes.len() } else { 0 };

        let start = buf.len();
        buf.resize(start + total, 0);
        let out = &mut buf[start..];

        let header = Header::new(PacketType::EquipmentStatus, Flags::NONE);
        header.serialize(out);
        out[4] = self.device_type as u8;
        out[5] = flags;
        out[6] = self.switch_a;
        out[7] = self.switch_b;
        out[8] = self.connected as u8;

        if has_labels {
            let len = labels_bytes.len() as u16;
            out[9..11].copy_from_slice(&len.to_be_bytes());
            out[11..11 + labels_bytes.len()].copy_from_slice(labels_bytes);
        }
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::EquipmentStatus {
            bail!("expected EquipmentStatus, got {:?}", header.packet_type);
        }
        if buf.len() < Self::MIN_SIZE {
            bail!("equipment status too short: {} < {}", buf.len(), Self::MIN_SIZE);
        }
        let device_type = DeviceType::from_u8(buf[4])
            .ok_or_else(|| anyhow::anyhow!("unknown device type: 0x{:02X}", buf[4]))?;
        let flags = buf[5];
        let switch_a = buf[6];
        let switch_b = buf[7];
        let connected = buf[8] != 0;

        let labels = if flags & EQUIPMENT_FLAG_HAS_LABELS != 0 && buf.len() >= Self::MIN_SIZE + 2 {
            let labels_len = u16::from_be_bytes(buf[9..11].try_into().unwrap()) as usize;
            if buf.len() >= 11 + labels_len {
                Some(String::from_utf8_lossy(&buf[11..11 + labels_len]).to_string())
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self { device_type, switch_a, switch_b, connected, labels })
    }
}

/// Amplitec power-cap tabel. Server stuurt de huidige tabel bij
/// client-connect en bij elke wijziging. Client stuurt dezelfde
/// struct terug wanneer de operator op "Save" drukt in de Amplitec-tab.
///
/// Wire: header(4) + 6 × { u16 max_w BE, u8 tx_blocked } = 22 bytes.
/// Index 0 = Amplitec-A positie 1, index 5 = positie 6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmplitecPowerTablePacket {
    /// Max forward watts per positie. `0` = geen cap (none).
    pub max_w: [u16; 6],
    /// TX-block per positie. `true` = RX-only, server staat geen TX toe.
    pub tx_blocked: [bool; 6],
}

impl AmplitecPowerTablePacket {
    pub const SIZE: usize = Header::SIZE + 6 * 3;

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::AmplitecPowerTable, Flags::NONE);
        header.serialize(&mut buf[..Header::SIZE]);
        for i in 0..6 {
            let off = Header::SIZE + i * 3;
            buf[off..off + 2].copy_from_slice(&self.max_w[i].to_be_bytes());
            buf[off + 2] = self.tx_blocked[i] as u8;
        }
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            bail!("AmplitecPowerTable too short: {} < {}", buf.len(), Self::SIZE);
        }
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::AmplitecPowerTable {
            bail!("expected AmplitecPowerTable, got {:?}", header.packet_type);
        }
        let mut max_w = [0u16; 6];
        let mut tx_blocked = [false; 6];
        for i in 0..6 {
            let off = Header::SIZE + i * 3;
            max_w[i] = u16::from_be_bytes(buf[off..off + 2].try_into().unwrap());
            tx_blocked[i] = buf[off + 2] != 0;
        }
        Ok(Self { max_w, tx_blocked })
    }
}

/// Equipment command packet: header(4) + device_type(1) + command_id(1) + data(N)
/// For Amplitec6x2: SetSwitchA(0x01)=[pos], SetSwitchB(0x02)=[pos]
#[derive(Debug, Clone)]
pub struct EquipmentCommandPacket {
    pub device_type: DeviceType,
    pub command_id: u8,
    pub data: Vec<u8>,
}

impl EquipmentCommandPacket {
    pub const MIN_SIZE: usize = Header::SIZE + 1 + 1; // 6 bytes

    pub const CMD_SET_SWITCH_A: u8 = 0x01;
    pub const CMD_SET_SWITCH_B: u8 = 0x02;

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let start = buf.len();
        buf.resize(start + Self::MIN_SIZE + self.data.len(), 0);
        let out = &mut buf[start..];

        let header = Header::new(PacketType::EquipmentCommand, Flags::NONE);
        header.serialize(out);
        out[4] = self.device_type as u8;
        out[5] = self.command_id;
        out[Self::MIN_SIZE..Self::MIN_SIZE + self.data.len()].copy_from_slice(&self.data);
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::EquipmentCommand {
            bail!("expected EquipmentCommand, got {:?}", header.packet_type);
        }
        if buf.len() < Self::MIN_SIZE {
            bail!("equipment command too short: {} < {}", buf.len(), Self::MIN_SIZE);
        }
        let device_type = DeviceType::from_u8(buf[4])
            .ok_or_else(|| anyhow::anyhow!("unknown device type: 0x{:02X}", buf[4]))?;
        let command_id = buf[5];
        let data = buf[Self::MIN_SIZE..].to_vec();
        Ok(Self { device_type, command_id, data })
    }
}

/// Yaesu radio state (server → client).
/// Format: [header(4)][freq_a(8)][freq_b(8)][mode(1)][smeter(2)][tx(1)][power_on(1)][af_gain(1)][tx_power(1)]
pub struct YaesuStatePacket {
    pub freq_a: u64,
    pub freq_b: u64,
    pub mode: u8,
    pub smeter: u16,
    pub tx_active: bool,
    pub power_on: bool,
    pub af_gain: u8,
    pub tx_power: u8,
    pub vfo_select: u8,    // 0=VFO, 1=Memory, 2=MemTune (from IF P7)
    pub memory_channel: u16, // Current memory channel (from IF)
    pub squelch: u8,       // 0-255
    pub rf_gain: u8,       // 0-255
    pub mic_gain: u8,      // 0-100
    pub split: bool,
    pub scan: bool,
}

impl YaesuStatePacket {
    pub const SIZE: usize = Header::SIZE + 8 + 8 + 1 + 2 + 1 + 1 + 1 + 1 + 1 + 2 + 1 + 1 + 1 + 1 + 1; // 35 bytes

    pub fn serialize(&self, buf: &mut [u8; Self::SIZE]) {
        let header = Header::new(PacketType::YaesuState, Flags::NONE);
        header.serialize(buf);
        let mut pos = Header::SIZE;
        buf[pos..pos + 8].copy_from_slice(&self.freq_a.to_be_bytes()); pos += 8;
        buf[pos..pos + 8].copy_from_slice(&self.freq_b.to_be_bytes()); pos += 8;
        buf[pos] = self.mode; pos += 1;
        buf[pos..pos + 2].copy_from_slice(&self.smeter.to_be_bytes()); pos += 2;
        buf[pos] = self.tx_active as u8; pos += 1;
        buf[pos] = self.power_on as u8; pos += 1;
        buf[pos] = self.af_gain; pos += 1;
        buf[pos] = self.tx_power; pos += 1;
        buf[pos] = self.vfo_select; pos += 1;
        buf[pos..pos + 2].copy_from_slice(&self.memory_channel.to_be_bytes()); pos += 2;
        buf[pos] = self.squelch; pos += 1;
        buf[pos] = self.rf_gain; pos += 1;
        buf[pos] = self.mic_gain; pos += 1;
        buf[pos] = self.split as u8; pos += 1;
        buf[pos] = self.scan as u8;
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        // Accept old 30-byte packets (without squelch) for backward compat
        if buf.len() < Self::SIZE - 1 {
            bail!("YaesuState packet too short: {} < {}", buf.len(), Self::SIZE - 1);
        }
        let mut pos = Header::SIZE;
        let freq_a = u64::from_be_bytes(buf[pos..pos + 8].try_into().unwrap()); pos += 8;
        let freq_b = u64::from_be_bytes(buf[pos..pos + 8].try_into().unwrap()); pos += 8;
        let mode = buf[pos]; pos += 1;
        let smeter = u16::from_be_bytes(buf[pos..pos + 2].try_into().unwrap()); pos += 2;
        let tx_active = buf[pos] != 0; pos += 1;
        let power_on = buf[pos] != 0; pos += 1;
        let af_gain = buf[pos]; pos += 1;
        let tx_power = buf[pos]; pos += 1;
        let vfo_select = buf[pos]; pos += 1;
        let memory_channel = u16::from_be_bytes(buf[pos..pos + 2].try_into().unwrap()); pos += 2;
        let squelch = if buf.len() > pos { buf[pos] } else { 0 }; pos += 1;
        let rf_gain = if buf.len() > pos { buf[pos] } else { 0 }; pos += 1;
        let mic_gain = if buf.len() > pos { buf[pos] } else { 0 }; pos += 1;
        let split = if buf.len() > pos { buf[pos] != 0 } else { false }; pos += 1;
        let scan = if buf.len() > pos { buf[pos] != 0 } else { false };
        Ok(Self { freq_a, freq_b, mode, smeter, tx_active, power_on, af_gain, tx_power, vfo_select, memory_channel, squelch, rf_gain, mic_gain, split, scan })
    }
}

/// TX profile list with names (server → client).
/// Format: [header][count: u8][active: u8][len1: u8][name1: bytes]...
pub struct TxProfilesPacket {
    pub names: Vec<String>,
    pub active: u8,
}

impl TxProfilesPacket {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let count = self.names.len().min(255) as u8;
        let names_size: usize = self.names.iter()
            .take(count as usize)
            .map(|n| 1 + n.len().min(255))
            .sum();
        let total = Header::SIZE + 2 + names_size;

        let start = buf.len();
        buf.resize(start + total, 0);
        let out = &mut buf[start..];

        let header = Header::new(PacketType::TxProfiles, Flags::NONE);
        header.serialize(out);
        out[Header::SIZE] = count;
        out[Header::SIZE + 1] = self.active;

        let mut pos = Header::SIZE + 2;
        for name in self.names.iter().take(count as usize) {
            let bytes = name.as_bytes();
            let len = bytes.len().min(255);
            out[pos] = len as u8;
            pos += 1;
            out[pos..pos + len].copy_from_slice(&bytes[..len]);
            pos += len;
        }
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        if header.packet_type != PacketType::TxProfiles {
            bail!("expected TxProfiles packet, got {:?}", header.packet_type);
        }
        if buf.len() < Header::SIZE + 2 {
            bail!("TxProfiles packet too short");
        }
        let count = buf[Header::SIZE] as usize;
        let active = buf[Header::SIZE + 1];

        let mut pos = Header::SIZE + 2;
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            if pos >= buf.len() { break; }
            let len = buf[pos] as usize;
            pos += 1;
            if pos + len > buf.len() { break; }
            names.push(String::from_utf8_lossy(&buf[pos..pos + len]).to_string());
            pos += len;
        }
        Ok(Self { names, active })
    }
}

/// Parse any incoming packet by peeking at the header
pub enum Packet {
    Audio(AudioPacket),
    Heartbeat(Heartbeat),
    HeartbeatAck(HeartbeatAck),
    Control(ControlPacket),
    Disconnect,
    PttDenied,
    Frequency(FrequencyPacket),
    Mode(ModePacket),
    Smeter(SmeterPacket),
    Spectrum(SpectrumPacket),
    FullSpectrum(SpectrumPacket),
    EquipmentStatus(EquipmentStatusPacket),
    EquipmentCommand(EquipmentCommandPacket),
    AmplitecPowerTable(AmplitecPowerTablePacket),
    AudioRx2(AudioPacket),
    FrequencyRx2(FrequencyPacket),
    ModeRx2(ModePacket),
    SmeterRx2(SmeterPacket),
    SmeterSig(SmeterPacket),
    SmeterMaxBin(SmeterPacket),
    SmeterRx2Sig(SmeterPacket),
    SmeterRx2MaxBin(SmeterPacket),
    SpectrumRx2(SpectrumPacket),
    FullSpectrumRx2(SpectrumPacket),
    Spot(SpotPacket),
    TxProfiles(TxProfilesPacket),
    AudioYaesu(AudioPacket),
    AudioBinR(AudioPacket),
    AudioMultiCh(MultiChannelAudioPacket),
    YaesuState(YaesuStatePacket),
    FrequencyYaesu(FrequencyPacket),
    YaesuMemoryData(String),
    AuthChallenge([u8; 16]),    // nonce
    AuthResponse([u8; 32]),     // HMAC
    AuthResult(u8),             // 0=rejected, 1=accepted, 2=totp_required
    TotpChallenge,              // server requests TOTP code
    TotpResponse(String),       // 6-digit TOTP code
}

/// AuthResult codes
pub const AUTH_REJECTED: u8 = 0;
pub const AUTH_ACCEPTED: u8 = 1;
pub const AUTH_TOTP_REQUIRED: u8 = 2;

impl Packet {
    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        let header = Header::deserialize(buf)?;
        match header.packet_type {
            PacketType::Audio => Ok(Packet::Audio(AudioPacket::deserialize(buf)?)),
            PacketType::Heartbeat => Ok(Packet::Heartbeat(Heartbeat::deserialize(buf)?)),
            PacketType::HeartbeatAck => Ok(Packet::HeartbeatAck(HeartbeatAck::deserialize(buf)?)),
            PacketType::Control => Ok(Packet::Control(ControlPacket::deserialize(buf)?)),
            PacketType::Disconnect => Ok(Packet::Disconnect),
            PacketType::PttDenied => Ok(Packet::PttDenied),
            PacketType::Frequency => Ok(Packet::Frequency(FrequencyPacket::deserialize(buf)?)),
            PacketType::Mode => Ok(Packet::Mode(ModePacket::deserialize(buf)?)),
            PacketType::Smeter => Ok(Packet::Smeter(SmeterPacket::deserialize(buf)?)),
            PacketType::SmeterSig => Ok(Packet::SmeterSig(SmeterPacket::deserialize(buf)?)),
            PacketType::SmeterMaxBin => Ok(Packet::SmeterMaxBin(SmeterPacket::deserialize(buf)?)),
            PacketType::SmeterRx2Sig => Ok(Packet::SmeterRx2Sig(SmeterPacket::deserialize(buf)?)),
            PacketType::SmeterRx2MaxBin => Ok(Packet::SmeterRx2MaxBin(SmeterPacket::deserialize(buf)?)),
            PacketType::Spectrum => Ok(Packet::Spectrum(SpectrumPacket::deserialize(buf)?)),
            PacketType::FullSpectrum => Ok(Packet::FullSpectrum(SpectrumPacket::deserialize(buf)?)),
            PacketType::EquipmentStatus => Ok(Packet::EquipmentStatus(EquipmentStatusPacket::deserialize(buf)?)),
            PacketType::EquipmentCommand => Ok(Packet::EquipmentCommand(EquipmentCommandPacket::deserialize(buf)?)),
            PacketType::AmplitecPowerTable => Ok(Packet::AmplitecPowerTable(AmplitecPowerTablePacket::deserialize(buf)?)),
            PacketType::AudioRx2 => Ok(Packet::AudioRx2(AudioPacket::deserialize(buf)?)),
            PacketType::FrequencyRx2 => Ok(Packet::FrequencyRx2(FrequencyPacket::deserialize(buf)?)),
            PacketType::ModeRx2 => Ok(Packet::ModeRx2(ModePacket::deserialize(buf)?)),
            PacketType::SmeterRx2 => Ok(Packet::SmeterRx2(SmeterPacket::deserialize(buf)?)),
            PacketType::SpectrumRx2 => Ok(Packet::SpectrumRx2(SpectrumPacket::deserialize(buf)?)),
            PacketType::FullSpectrumRx2 => Ok(Packet::FullSpectrumRx2(SpectrumPacket::deserialize(buf)?)),
            PacketType::Spot => Ok(Packet::Spot(SpotPacket::deserialize(buf)?)),
            PacketType::TxProfiles => Ok(Packet::TxProfiles(TxProfilesPacket::deserialize(buf)?)),
            PacketType::AudioYaesu => Ok(Packet::AudioYaesu(AudioPacket::deserialize(buf)?)),
            PacketType::AudioBinR => Ok(Packet::AudioBinR(AudioPacket::deserialize(buf)?)),
            PacketType::AudioMultiCh => Ok(Packet::AudioMultiCh(MultiChannelAudioPacket::deserialize(buf)?)),
            PacketType::YaesuState => Ok(Packet::YaesuState(YaesuStatePacket::deserialize(buf)?)),
            PacketType::FrequencyYaesu => Ok(Packet::FrequencyYaesu(FrequencyPacket::deserialize(buf)?)),
            PacketType::AuthChallenge => {
                if buf.len() < 20 { bail!("AuthChallenge too short"); }
                let mut nonce = [0u8; 16];
                nonce.copy_from_slice(&buf[4..20]);
                Ok(Packet::AuthChallenge(nonce))
            }
            PacketType::AuthResponse => {
                if buf.len() < 36 { bail!("AuthResponse too short"); }
                let mut hmac = [0u8; 32];
                hmac.copy_from_slice(&buf[4..36]);
                Ok(Packet::AuthResponse(hmac))
            }
            PacketType::AuthResult => {
                if buf.len() < 5 { bail!("AuthResult too short"); }
                Ok(Packet::AuthResult(buf[4]))
            }
            PacketType::TotpChallenge => {
                Ok(Packet::TotpChallenge)
            }
            PacketType::TotpResponse => {
                if buf.len() < 6 { bail!("TotpResponse too short"); }
                let len = u16::from_be_bytes(buf[4..6].try_into().unwrap()) as usize;
                if buf.len() < 6 + len { bail!("TotpResponse truncated"); }
                let code = String::from_utf8_lossy(&buf[6..6+len]).to_string();
                Ok(Packet::TotpResponse(code))
            }
            PacketType::YaesuMemoryData => {
                if buf.len() < 6 { bail!("YaesuMemoryData too short"); }
                let len = u16::from_be_bytes(buf[4..6].try_into().unwrap()) as usize;
                if buf.len() < 6 + len { bail!("YaesuMemoryData truncated"); }
                let text = String::from_utf8_lossy(&buf[6..6+len]).to_string();
                Ok(Packet::YaesuMemoryData(text))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let header = Header::new(PacketType::Audio, Flags::PTT);
        let mut buf = [0u8; 4];
        header.serialize(&mut buf);

        assert_eq!(buf[0], MAGIC);
        assert_eq!(buf[1], VERSION);
        assert_eq!(buf[2], 0x01);
        assert_eq!(buf[3], 0x01);

        let decoded = Header::deserialize(&buf).unwrap();
        assert_eq!(decoded.packet_type, PacketType::Audio);
        assert!(decoded.flags.ptt());
    }

    #[test]
    fn header_invalid_magic() {
        let buf = [0x00, VERSION, 0x01, 0x00];
        assert!(Header::deserialize(&buf).is_err());
    }

    #[test]
    fn audio_packet_roundtrip() {
        let packet = AudioPacket {
            flags: Flags::NONE,
            sequence: 42,
            timestamp: 12345,
            opus_data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let mut buf = Vec::new();
        packet.serialize(&mut buf);

        assert_eq!(buf.len(), AudioPacket::HEADER_SIZE + 4);

        let decoded = AudioPacket::deserialize(&buf).unwrap();
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.opus_data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(!decoded.flags.ptt());
    }

    #[test]
    fn audio_packet_with_ptt() {
        let packet = AudioPacket {
            flags: Flags::PTT,
            sequence: 1,
            timestamp: 0,
            opus_data: vec![0x01],
        };
        let mut buf = Vec::new();
        packet.serialize(&mut buf);

        let decoded = AudioPacket::deserialize(&buf).unwrap();
        assert!(decoded.flags.ptt());
    }

    #[test]
    fn heartbeat_roundtrip() {
        let hb = Heartbeat {
            flags: Flags::NONE,
            sequence: 100,
            local_time: 999_999,
            rtt_ms: 45,
            loss_percent: 2,
            jitter_ms: 10,
            capabilities: Capabilities::NONE.with(Capabilities::WIDEBAND_AUDIO),
        };
        let mut buf = [0u8; Heartbeat::SIZE];
        hb.serialize(&mut buf);

        assert_eq!(buf.len(), 20);

        let decoded = Heartbeat::deserialize(&buf).unwrap();
        assert_eq!(decoded.sequence, 100);
        assert_eq!(decoded.local_time, 999_999);
        assert_eq!(decoded.rtt_ms, 45);
        assert_eq!(decoded.loss_percent, 2);
        assert_eq!(decoded.jitter_ms, 10);
        assert!(decoded.capabilities.has(Capabilities::WIDEBAND_AUDIO));
    }

    #[test]
    fn heartbeat_backward_compat() {
        // Old 16-byte heartbeat without capabilities
        let mut buf = [0u8; 16];
        buf[0] = MAGIC;
        buf[1] = VERSION;
        buf[2] = PacketType::Heartbeat as u8;
        buf[3] = 0;
        buf[4..8].copy_from_slice(&42u32.to_be_bytes());
        let decoded = Heartbeat::deserialize(&buf).unwrap();
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.capabilities, Capabilities::NONE);
    }

    #[test]
    fn heartbeat_ack_roundtrip() {
        let ack = HeartbeatAck {
            flags: Flags::NONE,
            echo_sequence: 100,
            echo_time: 999_999,
            capabilities: Capabilities::NONE.with(Capabilities::WIDEBAND_AUDIO),
            state_flags: ServerStateFlags::NONE.with(ServerStateFlags::TCI_CONNECTED),
        };
        let mut buf = [0u8; HeartbeatAck::SIZE];
        ack.serialize(&mut buf);

        // SIZE is now 20 bytes (was 16 before state_flags was added in PATCH-1)
        assert_eq!(buf.len(), 20);
        assert_eq!(HeartbeatAck::SIZE, 20);

        let decoded = HeartbeatAck::deserialize(&buf).unwrap();
        assert_eq!(decoded.echo_sequence, 100);
        assert_eq!(decoded.echo_time, 999_999);
        assert!(decoded.capabilities.has(Capabilities::WIDEBAND_AUDIO));
        assert!(decoded.state_flags.has(ServerStateFlags::TCI_CONNECTED));
    }

    #[test]
    fn heartbeat_ack_backward_compat_12_byte() {
        // Old 12-byte HeartbeatAck (pre-Capabilities, pre-state_flags)
        let mut buf = [0u8; 12];
        buf[0] = MAGIC;
        buf[1] = VERSION;
        buf[2] = PacketType::HeartbeatAck as u8;
        buf[3] = 0;
        buf[4..8].copy_from_slice(&55u32.to_be_bytes());
        buf[8..12].copy_from_slice(&12345u32.to_be_bytes());
        let decoded = HeartbeatAck::deserialize(&buf).unwrap();
        assert_eq!(decoded.echo_sequence, 55);
        assert_eq!(decoded.echo_time, 12345);
        assert_eq!(decoded.capabilities, Capabilities::NONE);
        assert_eq!(decoded.state_flags, ServerStateFlags::NONE);
    }

    #[test]
    fn heartbeat_ack_intermediate_compat_16_byte() {
        // Intermediate 16-byte HeartbeatAck (capabilities present, state_flags absent —
        // this is what a v2.0.0 release-tag server sends, before PATCH-1).
        // The decoder must accept it and fill state_flags with NONE.
        let mut buf = [0u8; 16];
        buf[0] = MAGIC;
        buf[1] = VERSION;
        buf[2] = PacketType::HeartbeatAck as u8;
        buf[3] = 0;
        buf[4..8].copy_from_slice(&77u32.to_be_bytes());
        buf[8..12].copy_from_slice(&54321u32.to_be_bytes());
        buf[12..16].copy_from_slice(&Capabilities::WIDEBAND_AUDIO.to_be_bytes());
        let decoded = HeartbeatAck::deserialize(&buf).unwrap();
        assert_eq!(decoded.echo_sequence, 77);
        assert_eq!(decoded.echo_time, 54321);
        assert!(decoded.capabilities.has(Capabilities::WIDEBAND_AUDIO));
        // Key invariant: when state_flags absent, default to NONE — caller must
        // NOT assume "no TCI_CONNECTED bit = TCI down" (compatibility-regression check).
        assert_eq!(decoded.state_flags, ServerStateFlags::NONE);
    }

    #[test]
    fn packet_dispatch() {
        // Audio
        let audio = AudioPacket {
            flags: Flags::NONE,
            sequence: 1,
            timestamp: 0,
            opus_data: vec![0x00],
        };
        let mut buf = Vec::new();
        audio.serialize(&mut buf);
        assert!(matches!(Packet::deserialize(&buf).unwrap(), Packet::Audio(_)));

        // Heartbeat
        let hb = Heartbeat {
            flags: Flags::NONE,
            sequence: 1,
            local_time: 0,
            rtt_ms: 0,
            loss_percent: 0,
            jitter_ms: 0,
            capabilities: Capabilities::NONE,
        };
        let mut buf = [0u8; Heartbeat::SIZE];
        hb.serialize(&mut buf);
        assert!(matches!(
            Packet::deserialize(&buf).unwrap(),
            Packet::Heartbeat(_)
        ));

        // HeartbeatAck
        let ack = HeartbeatAck {
            flags: Flags::NONE,
            echo_sequence: 1,
            echo_time: 0,
            capabilities: Capabilities::NONE,
            state_flags: ServerStateFlags::NONE,
        };
        let mut buf = [0u8; HeartbeatAck::SIZE];
        ack.serialize(&mut buf);
        assert!(matches!(
            Packet::deserialize(&buf).unwrap(),
            Packet::HeartbeatAck(_)
        ));
    }

    #[test]
    fn flags_operations() {
        let f = Flags::NONE;
        assert!(!f.ptt());

        let f = f.with_ptt(true);
        assert!(f.ptt());
        assert_eq!(f, Flags::PTT);

        let f = f.with_ptt(false);
        assert!(!f.ptt());
        assert_eq!(f, Flags::NONE);
    }

    #[test]
    fn audio_packet_truncated() {
        let packet = AudioPacket {
            flags: Flags::NONE,
            sequence: 1,
            timestamp: 0,
            opus_data: vec![0x01, 0x02, 0x03],
        };
        let mut buf = Vec::new();
        packet.serialize(&mut buf);

        // Truncate: keep header but cut opus data
        buf.truncate(AudioPacket::HEADER_SIZE + 1);
        assert!(AudioPacket::deserialize(&buf).is_err());
    }
}
