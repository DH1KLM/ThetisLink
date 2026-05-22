// SPDX-License-Identifier: GPL-2.0-or-later

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Cosmetic alias for a StockCorner tuner — JC-3s and JC-4s share the same
/// MCP2221A-driven control protocol; the model name is only used for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunerModel {
    /// StockCorner JC-4s (default).
    Jc4s,
    /// StockCorner JC-3s.
    Jc3s,
}

impl TunerModel {
    /// Human-readable label for UI and logs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Jc4s => "JC-4s",
            Self::Jc3s => "JC-3s",
        }
    }
    /// Config-file token (lowercase, hyphenated).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Jc4s => "jc-4s",
            Self::Jc3s => "jc-3s",
        }
    }
    /// Parse a config-file token; unknown strings fall back to `Jc4s`.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "jc-3s" | "jc3s" => Self::Jc3s,
            _ => Self::Jc4s,
        }
    }
}

/// Per-slot tuner configuration. Two of these live in [`ServerConfig::tuners`].
#[derive(Clone, Debug)]
pub struct TunerConfig {
    /// Enable this tuner slot at server start.
    pub enabled: bool,
    /// Cosmetic model alias (JC-4s/JC-3s) used in the UI and logs only.
    pub model: TunerModel,
    /// USB serial of the MCP2221A board that drives this tuner.
    /// Empty string = "first available board on the bus" (legacy fallback,
    /// only sensible with a single tuner).
    pub mcp_serial: String,
    /// Amplitec-A antenna position (1..6) this tuner sits behind on the
    /// coax switch. `None` means the tuner is not bound to any Amplitec
    /// position; pressing Tune does not auto-route to it.
    pub amplitec_pos: Option<u8>,
    /// Tune-detector switch threshold on the yellow tune-status wire, in
    /// volts. Default 2.25 V (midpoint between the typical ~4.5 V idle and
    /// ~0 V LED-on level).
    pub threshold_v: f32,
    /// Hysteresis around the threshold, in volts. yellow_v <
    /// (threshold - hyst/2) counts as tune-active; yellow_v >
    /// (threshold + hyst/2) counts as tune-idle; samples inside the window
    /// preserve the previous state. Default 0.50 V.
    pub hysteresis_v: f32,
}

impl Default for TunerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: TunerModel::Jc4s,
            mcp_serial: String::new(),
            amplitec_pos: None,
            threshold_v: 2.25,
            hysteresis_v: 0.50,
        }
    }
}

/// Serializes all read-modify-write sequences on the server config file.
/// Without it the UI thread (active_pa toggle, save_window_positions,
/// start_server) and the RF2K poll thread (save_saved_drive) can race:
/// each loads, modifies its field, writes — and the later writer silently
/// drops the other's update if their load-windows overlap.
static CONFIG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub struct ServerConfig {
    /// TCI WebSocket address (e.g. "127.0.0.1:40001")
    pub tci_addr: Option<String>,
    pub spectrum_enabled: bool,
    /// Path to Thetis.exe for auto-launch (None = disabled)
    pub thetis_path: Option<String>,
    /// Yaesu FT-991A serial port (e.g. "COM8")
    pub yaesu_port: Option<String>,
    pub yaesu_enabled: bool,
    pub yaesu_baud: u32,
    /// Yaesu USB audio input device name pattern (e.g. "USB Audio")
    pub yaesu_audio_device: Option<String>,
    /// Amplitec 6/2 serial port (e.g. "COM3")
    pub amplitec_port: Option<String>,
    pub amplitec_enabled: bool,
    /// Labels for Amplitec antenna positions 1-6 (shared by A and B)
    pub amplitec_labels: [String; 6],
    /// Show Amplitec control window on start (default true)
    pub show_amplitec_window: bool,
    /// Per-slot StockCorner tuner configuration. Index 0 = tuner 1, index 1 =
    /// tuner 2. Each slot is independently enabled, has its own model alias
    /// (cosmetic), targets a specific MCP2221A board by USB serial, and may be
    /// bound to a specific Amplitec-A antenna position so the server knows
    /// which tuner to drive when the user presses Tune. **Schema is wired into
    /// load/save now; the runtime + UI that consume it land in a follow-up
    /// patch, alongside the MCP2221A tuner-driver rewrite.**
    pub tuners: [TunerConfig; 2],
    /// Show tuner control window on start (default true)
    pub show_tuner_window: bool,
    /// SPE Expert 1.3K-FA serial port (e.g. "COM6")
    pub spe_port: Option<String>,
    pub spe_enabled: bool,
    /// Show SPE Expert control window on start (default true)
    pub show_spe_window: bool,
    /// RF2K-S Raspberry Pi address (e.g. "192.168.1.50:8080")
    pub rf2k_addr: Option<String>,
    pub rf2k_enabled: bool,
    /// Show RF2K-S control window on start (default true)
    pub show_rf2k_window: bool,
    /// UltraBeam RCU-06 serial port (e.g. "COM7")
    pub ultrabeam_port: Option<String>,
    pub ultrabeam_enabled: bool,
    /// Show UltraBeam control window on start (default true)
    pub show_ultrabeam_window: bool,
    /// EA7HG Visual Rotor TCP address (e.g. "192.168.1.60:3010")
    pub rotor_addr: Option<String>,
    pub rotor_enabled: bool,
    /// Show Rotor control window on start (default true)
    pub show_rotor_window: bool,
    /// Saved window positions: [x, y]
    pub tuner_window_pos: Option<[f32; 2]>,
    pub amplitec_window_pos: Option<[f32; 2]>,
    pub spe_window_pos: Option<[f32; 2]>,
    pub rf2k_window_pos: Option<[f32; 2]>,
    pub ultrabeam_window_pos: Option<[f32; 2]>,
    pub rotor_window_pos: Option<[f32; 2]>,
    /// Saved main window position: [x, y]
    pub main_window_pos: Option<[f32; 2]>,
    /// Saved window sizes: [w, h]
    pub main_window_size: Option<[f32; 2]>,
    pub tuner_window_size: Option<[f32; 2]>,
    pub amplitec_window_size: Option<[f32; 2]>,
    pub spe_window_size: Option<[f32; 2]>,
    pub rf2k_window_size: Option<[f32; 2]>,
    pub ultrabeam_window_size: Option<[f32; 2]>,
    pub rotor_window_size: Option<[f32; 2]>,
    /// Auto-start server on launch (skip settings screen)
    pub autostart: bool,
    /// Active PA: 0=none, 1=SPE, 2=RF2K
    pub active_pa: u8,
    /// Persisted pre-Operate Thetis ZZPC drive level per PA. `Some(n)` is the
    /// last value captured when the PA went Standby→Operate; `None` means
    /// no snapshot has been taken yet. Used by the TL2-server drive observer
    /// to restore the value when the PA goes Operate→Standby, even across a
    /// host restart that would otherwise lose the in-memory observer state.
    pub rf2k_saved_drive: Option<u8>,
    pub spe_saved_drive: Option<u8>,
    /// UltraBeam control window: Menu section expanded (collapsible state)
    pub ultrabeam_show_menu: bool,
    /// Status panel: MCP2221A tuner-bridges section expanded. Persisted so
    /// the owner's last open/closed choice survives a server restart.
    pub mcp2221_section_expanded: bool,
    /// DX Cluster telnet server address (e.g. "dxc.pi4cc.nl:8000")
    pub dxcluster_server: String,
    /// DX Cluster callsign for login
    pub dxcluster_callsign: String,
    /// DX Cluster enabled
    pub dxcluster_enabled: bool,
    /// DX Cluster spot expiry time in minutes (default 10)
    pub dxcluster_expiry_min: u16,
    /// Network authentication password (None = no auth, any client can connect)
    pub password: Option<String>,
    /// TOTP 2FA secret (base32, None = 2FA disabled)
    pub totp_secret: Option<String>,
    /// TOTP 2FA enabled
    pub totp_enabled: bool,
    /// PATCH-3: human-readable name advertised via mDNS so clients can
    /// distinguish multiple ThetisLink servers on the same LAN
    /// (e.g. "Shack PC"). `None` falls back to the OS hostname.
    pub friendly_name: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tci_addr: None,
            spectrum_enabled: true,
            thetis_path: detect_thetis_path(),
            yaesu_port: None,
            yaesu_enabled: false,
            yaesu_baud: 38400,
            yaesu_audio_device: None,
            amplitec_port: None,
            amplitec_enabled: true,
            amplitec_labels: default_labels("Ant"),
            show_amplitec_window: true,
            tuners: [TunerConfig::default(), TunerConfig::default()],
            show_tuner_window: true,
            spe_port: None,
            spe_enabled: true,
            show_spe_window: true,
            rf2k_addr: None,
            rf2k_enabled: true,
            show_rf2k_window: true,
            ultrabeam_port: None,
            ultrabeam_enabled: true,
            show_ultrabeam_window: true,
            rotor_addr: None,
            rotor_enabled: true,
            show_rotor_window: true,
            tuner_window_pos: None,
            amplitec_window_pos: None,
            spe_window_pos: None,
            rf2k_window_pos: None,
            ultrabeam_window_pos: None,
            rotor_window_pos: None,
            main_window_pos: None,
            main_window_size: None,
            tuner_window_size: None,
            amplitec_window_size: None,
            spe_window_size: None,
            rf2k_window_size: None,
            ultrabeam_window_size: None,
            rotor_window_size: None,
            autostart: false,
            active_pa: 0,
            rf2k_saved_drive: None,
            spe_saved_drive: None,
            ultrabeam_show_menu: false,
            mcp2221_section_expanded: true,
            dxcluster_server: "dxc.pi4cc.nl:8000".to_string(),
            dxcluster_callsign: "PA3GHM".to_string(),
            dxcluster_enabled: true,
            dxcluster_expiry_min: 10,
            password: None,
            totp_secret: None,
            totp_enabled: false,
            friendly_name: None,
        }
    }
}

/// Apply a `tuner1_FIELD=value` / `tuner2_FIELD=value` config entry to the
/// matching `TunerConfig` slot.  Unknown sub-keys are silently ignored so a
/// future field-name doesn't have to be cross-version-compatible.
fn parse_tuner_key(t: &mut TunerConfig, sub: &str, value: &str) {
    match sub {
        "enabled" => t.enabled = value != "false",
        "model" => t.model = TunerModel::parse(value),
        "mcp_serial" => t.mcp_serial = value.to_string(),
        "amplitec_pos" => {
            t.amplitec_pos = match value.parse::<u8>().ok() {
                Some(n) if (1..=6).contains(&n) => Some(n),
                _ => None,
            };
        }
        "threshold_v" => {
            if let Ok(v) = value.parse::<f32>() {
                t.threshold_v = v.clamp(0.5, 4.5);
            }
        }
        "hysteresis_v" => {
            if let Ok(v) = value.parse::<f32>() {
                t.hysteresis_v = v.clamp(0.1, 2.0);
            }
        }
        _ => {}
    }
}

fn default_labels(prefix: &str) -> [String; 6] {
    [
        format!("{prefix}1"), format!("{prefix}2"), format!("{prefix}3"),
        format!("{prefix}4"), format!("{prefix}5"), format!("{prefix}6"),
    ]
}

fn detect_thetis_path() -> Option<String> {
    let default = r"C:\Program Files\OpenHPSDR\Thetis\Thetis.exe";
    if std::path::Path::new(default).exists() {
        Some(default.to_string())
    } else {
        None
    }
}

fn config_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .unwrap_or(std::path::Path::new("."))
        .join("thetislink-server.conf")
}

/// Public load: takes the global CONFIG_LOCK so external callers always see
/// a consistent snapshot — never the half-written file from a concurrent
/// save. Internal `load_unlocked` is for `modify_config` which already
/// holds the lock.
pub fn load() -> ServerConfig {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked()
}

fn load_unlocked() -> ServerConfig {
    let path = config_path();
    let mut config = ServerConfig::default();

    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "tci" => {
                        let v = value.trim().to_string();
                        config.tci_addr = if v.is_empty() { None } else { Some(v) };
                    }
                    // Legacy keys (ignored, kept for backward compat with old config files)
                    "input" | "input2" | "output" | "anan_interface" => {}
                    "thetis_path" => {
                        let v = value.trim().to_string();
                        if v.is_empty() {
                            config.thetis_path = None;
                        } else {
                            config.thetis_path = Some(v);
                        }
                    }
                    "yaesu_port" => {
                        let v = value.trim().to_string();
                        config.yaesu_port = if v.is_empty() { None } else { Some(v) };
                    }
                    "yaesu_enabled" => {
                        config.yaesu_enabled = value.trim() != "false";
                    }
                    "yaesu_baud" => {
                        if let Ok(v) = value.trim().parse::<u32>() {
                            config.yaesu_baud = v;
                        }
                    }
                    "yaesu_audio" => {
                        let v = value.trim().to_string();
                        config.yaesu_audio_device = if v.is_empty() { None } else { Some(v) };
                    }
                    "amplitec_port" => {
                        let v = value.trim().to_string();
                        config.amplitec_port = if v.is_empty() { None } else { Some(v) };
                    }
                    "amplitec_enabled" => {
                        config.amplitec_enabled = value.trim() != "false";
                    }
                    k if k.starts_with("amplitec_label") => {
                        if let Some(idx) = k.strip_prefix("amplitec_label").and_then(|s| s.parse::<usize>().ok()) {
                            if idx >= 1 && idx <= 6 {
                                config.amplitec_labels[idx - 1] = value.trim().to_string();
                            }
                        }
                    }
                    // Backward compat: read old amplitec_aN keys as shared labels
                    k if k.starts_with("amplitec_a") => {
                        if let Some(idx) = k.strip_prefix("amplitec_a").and_then(|s| s.parse::<usize>().ok()) {
                            if idx >= 1 && idx <= 6 {
                                config.amplitec_labels[idx - 1] = value.trim().to_string();
                            }
                        }
                    }
                    k if k.starts_with("amplitec_b") => {
                        // Old amplitec_bN keys: ignore (same antennas)
                    }
                    "amplitec_window" => {
                        config.show_amplitec_window = value.trim() == "true";
                    }
                    // Legacy v2.0.2 keys silently ignored on load (multi-tuner
                    // runtime supersedes the single serial-port flow):
                    //   `tuner_port`           — COM-port no longer used
                    //   `tuner_enabled`        — replaced by per-slot enable
                    //   `tuner_assume_tuned`   — assume-tuned pad retired
                    "tuner_port" => {}
                    "tuner_enabled" => {
                        // Honour the legacy global toggle one last time so
                        // owners upgrading from v2.0.2 do not lose their
                        // slot-0 enable state: mirror into tuners[0].enabled.
                        let v = value.trim() != "false";
                        config.tuners[0].enabled = v;
                    }
                    "tuner_assume_tuned" => {}
                    // New per-slot keys: tuner1_* / tuner2_* dispatched to
                    // config.tuners[0] / config.tuners[1] respectively.
                    k if k.starts_with("tuner1_") => {
                        parse_tuner_key(&mut config.tuners[0], &k[7..], value.trim());
                    }
                    k if k.starts_with("tuner2_") => {
                        parse_tuner_key(&mut config.tuners[1], &k[7..], value.trim());
                    }
                    "tuner_window" => {
                        config.show_tuner_window = value.trim() == "true";
                    }
                    "tuner_safe_drive" => {
                        // Legacy key, ignored
                    }
                    "tuner_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.tuner_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "tuner_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.tuner_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "amplitec_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.amplitec_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "amplitec_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.amplitec_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "spe_port" => {
                        let v = value.trim().to_string();
                        config.spe_port = if v.is_empty() { None } else { Some(v) };
                    }
                    "spe_enabled" => {
                        config.spe_enabled = value.trim() != "false";
                    }
                    "spe_window" => {
                        config.show_spe_window = value.trim() == "true";
                    }
                    "spe_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.spe_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "spe_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.spe_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "rf2k_addr" => {
                        let v = value.trim().to_string();
                        config.rf2k_addr = if v.is_empty() { None } else { Some(v) };
                    }
                    "rf2k_enabled" => {
                        config.rf2k_enabled = value.trim() != "false";
                    }
                    "rf2k_window" => {
                        config.show_rf2k_window = value.trim() == "true";
                    }
                    "rf2k_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rf2k_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "rf2k_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rf2k_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "ultrabeam_port" => {
                        let v = value.trim().to_string();
                        config.ultrabeam_port = if v.is_empty() { None } else { Some(v) };
                    }
                    "ultrabeam_enabled" => {
                        config.ultrabeam_enabled = value.trim() != "false";
                    }
                    "ultrabeam_window" => {
                        config.show_ultrabeam_window = value.trim() == "true";
                    }
                    "ultrabeam_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.ultrabeam_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "ultrabeam_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.ultrabeam_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "rotor_addr" => {
                        let v = value.trim().to_string();
                        config.rotor_addr = if v.is_empty() { None } else { Some(v) };
                    }
                    "rotor_enabled" => {
                        config.rotor_enabled = value.trim() != "false";
                    }
                    "rotor_window" => {
                        config.show_rotor_window = value.trim() == "true";
                    }
                    "rotor_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rotor_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "rotor_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rotor_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "main_pos_x" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.main_window_pos.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "main_pos_y" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.main_window_pos.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "main_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.main_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "main_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.main_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "tuner_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.tuner_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "tuner_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.tuner_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "amplitec_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.amplitec_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "amplitec_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.amplitec_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "spe_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.spe_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "spe_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.spe_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "rf2k_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rf2k_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "rf2k_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rf2k_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "ultrabeam_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.ultrabeam_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "ultrabeam_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.ultrabeam_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "rotor_size_w" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rotor_window_size.get_or_insert([0.0, 0.0])[0] = v;
                        }
                    }
                    "rotor_size_h" => {
                        if let Ok(v) = value.trim().parse::<f32>() {
                            config.rotor_window_size.get_or_insert([0.0, 0.0])[1] = v;
                        }
                    }
                    "autostart" => {
                        config.autostart = value.trim() == "true";
                    }
                    "active_pa" => {
                        config.active_pa = value.trim().parse().unwrap_or(0);
                    }
                    "rf2k_saved_drive" => {
                        let v = value.trim();
                        config.rf2k_saved_drive = if v.is_empty() { None } else { v.parse().ok() };
                    }
                    "spe_saved_drive" => {
                        let v = value.trim();
                        config.spe_saved_drive = if v.is_empty() { None } else { v.parse().ok() };
                    }
                    "ultrabeam_show_menu" => {
                        config.ultrabeam_show_menu = value.trim() == "true";
                    }
                    "mcp2221_section_expanded" => {
                        config.mcp2221_section_expanded = value.trim() != "false";
                    }
                    "dxcluster_server" => {
                        let v = value.trim().to_string();
                        if !v.is_empty() { config.dxcluster_server = v; }
                    }
                    "dxcluster_callsign" => {
                        let v = value.trim().to_string();
                        if !v.is_empty() { config.dxcluster_callsign = v; }
                    }
                    "dxcluster_enabled" => {
                        config.dxcluster_enabled = value.trim() != "false";
                    }
                    "dxcluster_expiry_min" => {
                        if let Ok(v) = value.trim().parse::<u16>() {
                            config.dxcluster_expiry_min = v.max(1);
                        }
                    }
                    "password" => {
                        let v = value.trim();
                        if !v.is_empty() {
                            // Try deobfuscate first; if it fails, treat as plaintext (first time)
                            config.password = Some(
                                sdr_remote_core::auth::deobfuscate_password(v)
                                    .unwrap_or_else(|| v.to_string())
                            );
                        }
                    }
                    "totp_secret" => {
                        let v = value.trim();
                        if !v.is_empty() {
                            config.totp_secret = Some(
                                sdr_remote_core::auth::deobfuscate_password(v)
                                    .unwrap_or_else(|| v.to_string())
                            );
                        }
                    }
                    "totp_enabled" => {
                        config.totp_enabled = value.trim() == "true";
                    }
                    "friendly_name" => {
                        let v = value.trim();
                        if !v.is_empty() {
                            config.friendly_name = Some(v.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    config
}

/// Atomic load-modify-save helper. All read-modify-write helpers funnel
/// through here so the CONFIG_LOCK guarantees no other writer slips in
/// between the load and the save.
pub fn modify_config(f: impl FnOnce(&mut ServerConfig)) {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut config = load_unlocked();
    f(&mut config);
    save_unlocked(&config);
}

/// Read-modify-write helper for the `active_pa` config key. Used when the
/// user clicks an Active checkbox on RF2K or SPE so the choice survives a
/// non-graceful shutdown (process kill / power loss) without waiting for
/// `start_server()` to persist the rest of the config.
pub fn save_active_pa(value: u8) {
    modify_config(|c| c.active_pa = value);
}

/// Read-modify-write helper for the per-PA pre-Operate drive snapshot.
/// `pa_id`: 1 = SPE, 2 = RF2K (matching the `active_pa` encoding).
/// `value`: `Some(zzpc)` to write, `None` to clear.
pub fn save_saved_drive(pa_id: u8, value: Option<u8>) {
    modify_config(|c| match pa_id {
        1 => c.spe_saved_drive = value,
        2 => c.rf2k_saved_drive = value,
        _ => {}
    });
}

/// Read-modify-write helper for the UltraBeam window Menu collapsible state.
pub fn save_ultrabeam_show_menu(value: bool) {
    modify_config(|c| c.ultrabeam_show_menu = value);
}

/// Read-modify-write helper for the MCP2221A tuner-bridges section
/// collapsible state in the Status panel.
pub fn save_mcp2221_section_expanded(value: bool) {
    modify_config(|c| c.mcp2221_section_expanded = value);
}

/// Public save: takes the global CONFIG_LOCK so writes are serialised with
/// reads and other writes. Internal `save_unlocked` is for `modify_config`
/// which already holds the lock.
pub fn save(config: &ServerConfig) {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    save_unlocked(config);
}

fn save_unlocked(config: &ServerConfig) {
    let path = config_path();
    let mut contents = format!(
        "tci={}\nthetis_path={}\nyaesu_port={}\nyaesu_enabled={}\nyaesu_baud={}\nyaesu_audio={}\namplitec_port={}\namplitec_enabled={}\namplitec_window={}\ntuner_window={}\nspe_port={}\nspe_enabled={}\nspe_window={}\nrf2k_addr={}\nrf2k_enabled={}\nrf2k_window={}\nultrabeam_port={}\nultrabeam_enabled={}\nultrabeam_window={}\nrotor_addr={}\nrotor_enabled={}\nrotor_window={}\n",
        config.tci_addr.as_deref().unwrap_or(""),
        config.thetis_path.as_deref().unwrap_or(""),
        config.yaesu_port.as_deref().unwrap_or(""),
        config.yaesu_enabled,
        config.yaesu_baud,
        config.yaesu_audio_device.as_deref().unwrap_or(""),
        config.amplitec_port.as_deref().unwrap_or(""),
        config.amplitec_enabled,
        config.show_amplitec_window,
        config.show_tuner_window,
        config.spe_port.as_deref().unwrap_or(""),
        config.spe_enabled,
        config.show_spe_window,
        config.rf2k_addr.as_deref().unwrap_or(""),
        config.rf2k_enabled,
        config.show_rf2k_window,
        config.ultrabeam_port.as_deref().unwrap_or(""),
        config.ultrabeam_enabled,
        config.show_ultrabeam_window,
        config.rotor_addr.as_deref().unwrap_or(""),
        config.rotor_enabled,
        config.show_rotor_window,
    );
    // Per-tuner slots — tuner1_* / tuner2_* keys keep the file readable by hand.
    for (i, t) in config.tuners.iter().enumerate() {
        let prefix = format!("tuner{}", i + 1);
        contents.push_str(&format!("{}_enabled={}\n", prefix, t.enabled));
        contents.push_str(&format!("{}_model={}\n", prefix, t.model.as_str()));
        contents.push_str(&format!("{}_mcp_serial={}\n", prefix, t.mcp_serial));
        contents.push_str(&format!(
            "{}_amplitec_pos={}\n",
            prefix,
            t.amplitec_pos.map(|n| n.to_string()).unwrap_or_default()
        ));
        contents.push_str(&format!("{}_threshold_v={}\n", prefix, t.threshold_v));
        contents.push_str(&format!("{}_hysteresis_v={}\n", prefix, t.hysteresis_v));
    }
    for i in 0..6 {
        contents.push_str(&format!("amplitec_label{}={}\n", i + 1, config.amplitec_labels[i]));
    }
    if let Some(pos) = config.tuner_window_pos {
        contents.push_str(&format!("tuner_pos_x={}\ntuner_pos_y={}\n", pos[0], pos[1]));
    }
    if let Some(pos) = config.amplitec_window_pos {
        contents.push_str(&format!("amplitec_pos_x={}\namplitec_pos_y={}\n", pos[0], pos[1]));
    }
    if let Some(pos) = config.spe_window_pos {
        contents.push_str(&format!("spe_pos_x={}\nspe_pos_y={}\n", pos[0], pos[1]));
    }
    if let Some(pos) = config.rf2k_window_pos {
        contents.push_str(&format!("rf2k_pos_x={}\nrf2k_pos_y={}\n", pos[0], pos[1]));
    }
    if let Some(pos) = config.ultrabeam_window_pos {
        contents.push_str(&format!("ultrabeam_pos_x={}\nultrabeam_pos_y={}\n", pos[0], pos[1]));
    }
    if let Some(pos) = config.rotor_window_pos {
        contents.push_str(&format!("rotor_pos_x={}\nrotor_pos_y={}\n", pos[0], pos[1]));
    }
    // Main window position
    if let Some(pos) = config.main_window_pos {
        contents.push_str(&format!("main_pos_x={}\nmain_pos_y={}\n", pos[0], pos[1]));
    }
    // Window sizes
    if let Some(sz) = config.main_window_size {
        contents.push_str(&format!("main_size_w={}\nmain_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.tuner_window_size {
        contents.push_str(&format!("tuner_size_w={}\ntuner_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.amplitec_window_size {
        contents.push_str(&format!("amplitec_size_w={}\namplitec_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.spe_window_size {
        contents.push_str(&format!("spe_size_w={}\nspe_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.rf2k_window_size {
        contents.push_str(&format!("rf2k_size_w={}\nrf2k_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.ultrabeam_window_size {
        contents.push_str(&format!("ultrabeam_size_w={}\nultrabeam_size_h={}\n", sz[0], sz[1]));
    }
    if let Some(sz) = config.rotor_window_size {
        contents.push_str(&format!("rotor_size_w={}\nrotor_size_h={}\n", sz[0], sz[1]));
    }
    contents.push_str(&format!("autostart={}\n", config.autostart));
    contents.push_str(&format!("active_pa={}\n", config.active_pa));
    if let Some(v) = config.rf2k_saved_drive {
        contents.push_str(&format!("rf2k_saved_drive={}\n", v));
    }
    if let Some(v) = config.spe_saved_drive {
        contents.push_str(&format!("spe_saved_drive={}\n", v));
    }
    contents.push_str(&format!("ultrabeam_show_menu={}\n", config.ultrabeam_show_menu));
    contents.push_str(&format!("mcp2221_section_expanded={}\n", config.mcp2221_section_expanded));
    contents.push_str(&format!("dxcluster_server={}\n", config.dxcluster_server));
    contents.push_str(&format!("dxcluster_callsign={}\n", config.dxcluster_callsign));
    contents.push_str(&format!("dxcluster_enabled={}\n", config.dxcluster_enabled));
    contents.push_str(&format!("dxcluster_expiry_min={}\n", config.dxcluster_expiry_min));
    if let Some(ref pw) = config.password {
        contents.push_str(&format!("password={}\n", sdr_remote_core::auth::obfuscate_password(pw)));
    }
    contents.push_str(&format!("totp_enabled={}\n", config.totp_enabled));
    if let Some(ref secret) = config.totp_secret {
        contents.push_str(&format!("totp_secret={}\n", sdr_remote_core::auth::obfuscate_password(secret)));
    }
    if let Some(ref name) = config.friendly_name {
        contents.push_str(&format!("friendly_name={}\n", name));
    }
    let _ = fs::write(&path, contents);
}

/// Format labels as comma-separated string for protocol transmission.
/// Sends same labels twice (A and B share the same 6 antennas).
pub fn labels_string(config: &ServerConfig) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(12);
    for l in &config.amplitec_labels {
        parts.push(l);
    }
    // Duplicate for B (same antennas)
    for l in &config.amplitec_labels {
        parts.push(l);
    }
    parts.join(",")
}
