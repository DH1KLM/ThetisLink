// SPDX-License-Identifier: GPL-2.0-or-later

//! Compact server-state panel for the GUI (PATCH-2).
//!
//! Renders six elements in one screenful so the owner can answer
//! support questions ("is the server listening?", "what does it see?")
//! with a single screenshot:
//!
//! 1. Bind address + port
//! 2. TCI connection state + uptime
//! 3. Active clients (IP, RTT, since)
//! 4. Audio routing (per-channel activity + cumulative frame count)
//! 5. Recent connect attempts (ringbuffer N=10, success + failure)
//! 6. External devices (compact dot grid)
//!
//! All reads are non-blocking: SessionManager via `try_lock()` (returns
//! stale snapshot on contention — 1Hz refresh), audio/TCI via lock-free
//! atomics. UI never blocks the network or audio loops.

use egui::{Color32, RichText};

use crate::audio_stats::StatusPanelShared;

/// Render the Status panel into the provided `ui`. Designed to fit in
/// the same scrollable area as the existing log view; caller is
/// responsible for the surrounding ScrollArea.
pub fn render_status_panel(
    ui: &mut egui::Ui,
    shared: &StatusPanelShared,
    pending_bind_addr: &str,
    yaesu_configured: bool,
    amplitec_configured: bool,
    tuner_configured: bool,
    spe_configured: bool,
    rf2k_configured: bool,
) {
    let server_start = shared.server_start;

    // ── 1. Bind address ──────────────────────────────────────────────────
    // Reflect the actual bind outcome — a hardcoded placeholder would lie
    // about the listen address when the bind itself failed.
    ui.horizontal(|ui| {
        ui.label(RichText::new("Server:").strong());
        match shared.bind_status.get() {
            Some(crate::audio_stats::BindStatus::Ok { addr }) => {
                ui.colored_label(
                    Color32::from_rgb(50, 200, 50),
                    format!("Listening on UDP {}", addr),
                );
            }
            Some(crate::audio_stats::BindStatus::Failed { addr, error }) => {
                ui.colored_label(
                    Color32::from_rgb(220, 60, 60),
                    format!("Bind failed on {}: {}", addr, error),
                );
            }
            None => {
                ui.colored_label(
                    Color32::from_rgb(200, 160, 40),
                    format!("Binding {} …", pending_bind_addr),
                );
            }
        }
    });

    // ── 2. TCI connection ────────────────────────────────────────────────
    let (tci_connected, tci_age) = shared.tci.snapshot(server_start);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Thetis:").strong());
        if tci_connected {
            let dur = tci_age
                .map(format_duration_short)
                .unwrap_or_else(|| "—".to_string());
            ui.colored_label(
                Color32::from_rgb(50, 200, 50),
                format!("Connected  ({})", dur),
            );
        } else {
            let suffix = tci_age
                .map(|s| format!("for {}", format_duration_short(s)))
                .unwrap_or_else(|| "since startup".to_string());
            ui.colored_label(
                Color32::from_rgb(200, 60, 60),
                format!("Disconnected  ({})", suffix),
            );
        }
    });

    // ── 3. Active clients (snapshot via SessionManager.try_lock) ─────────
    let clients_snapshot = if let Some(session_arc) = shared.session_slot.get() {
        match session_arc.try_lock() {
            Ok(guard) => Some(guard.active_clients_snapshot()),
            Err(_) => None, // contended — show stale "—" rather than block UI
        }
    } else {
        None // server not yet ready
    };

    match &clients_snapshot {
        Some(clients) if clients.is_empty() => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Active clients:").strong());
                ui.label("0");
            });
        }
        Some(clients) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Active clients:").strong());
                ui.label(format!("{}", clients.len()));
            });
            for c in clients {
                let last_seen_age = c.last_seen.elapsed().as_secs();
                let connected_for =
                    format_duration_short(c.connected_since.elapsed().as_secs_f32());
                let line = format!(
                    "  {:<22}  connected {:<8}  rtt={}ms  loss={}%  jitter={}ms   (seen {}s ago)",
                    c.addr.to_string(),
                    connected_for,
                    c.rtt_ms,
                    c.loss_percent,
                    c.jitter_ms,
                    last_seen_age
                );
                let color = if !c.authenticated {
                    Color32::from_rgb(200, 160, 40) // authenticating
                } else if last_seen_age > 5 {
                    Color32::from_rgb(200, 160, 40) // stale
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(color, RichText::new(line).monospace());
            }
        }
        None => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Active clients:").strong());
                ui.colored_label(
                    Color32::from_rgb(160, 160, 160),
                    "(snapshot busy — updating…)",
                );
            });
        }
    }

    // ── 4. Audio routing ─────────────────────────────────────────────────
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Audio:").strong());
        render_channel_chip(ui, "RX1", &shared.audio.rx1, server_start);
        render_channel_chip(ui, "RX2", &shared.audio.rx2, server_start);
        render_channel_chip(ui, "TX", &shared.audio.tx, server_start);
        if yaesu_configured {
            render_channel_chip(ui, "Y-RX", &shared.audio.yaesu_rx, server_start);
            render_channel_chip(ui, "Y-TX", &shared.audio.yaesu_tx, server_start);
        }
    });

    // ── 5. Recent connect attempts ───────────────────────────────────────
    let attempts = if let Some(session_arc) = shared.session_slot.get() {
        match session_arc.try_lock() {
            Ok(guard) => Some(guard.recent_connect_attempts()),
            Err(_) => None,
        }
    } else {
        None
    };
    ui.separator();
    ui.label(RichText::new("Recent connect attempts:").strong());
    match &attempts {
        Some(list) if list.is_empty() => {
            ui.colored_label(
                Color32::from_rgb(160, 160, 160),
                "  (none yet)",
            );
        }
        Some(list) => {
            // Newest first for readability.
            for a in list.iter().rev() {
                let time_str = a.wall_clock.format("%H:%M:%S").to_string();
                let line = format!(
                    "  {}   {:<22}   {}",
                    time_str,
                    a.remote_addr.to_string(),
                    a.outcome.label()
                );
                let color = match a.outcome {
                    crate::session::ConnectOutcome::Accepted => {
                        Color32::from_rgb(50, 200, 50)
                    }
                    crate::session::ConnectOutcome::TotpRequired
                    | crate::session::ConnectOutcome::ChallengeSent => {
                        ui.visuals().text_color()
                    }
                    crate::session::ConnectOutcome::WrongPassword
                    | crate::session::ConnectOutcome::WrongTotp => {
                        Color32::from_rgb(220, 60, 60)
                    }
                };
                ui.colored_label(color, RichText::new(line).monospace());
            }
        }
        None => {
            ui.colored_label(
                Color32::from_rgb(160, 160, 160),
                "  (snapshot busy — updating…)",
            );
        }
    }

    // ── 6. Configured external devices (compact dot grid) ────────────────
    // Dots reflect Some/None at server-start, NOT live link health — true
    // online/offline detection per device is out of scope for PATCH-2.
    // Labelled "Configured devices" so the screenshot reader is not
    // misled into thinking a green dot means the radio replied.
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Configured devices:").strong());
        render_device_dot(ui, "Yaesu", yaesu_configured);
        render_device_dot(ui, "Amplitec", amplitec_configured);
        render_device_dot(ui, "Tuner", tuner_configured);
        render_device_dot(ui, "SPE", spe_configured);
        render_device_dot(ui, "RF2K", rf2k_configured);
    });
}

fn render_channel_chip(
    ui: &mut egui::Ui,
    label: &str,
    stats: &crate::audio_stats::ChannelStats,
    server_start: std::time::Instant,
) {
    let (count, age) = stats.snapshot(server_start);
    let (sym, color) = match age {
        None => ("—", Color32::from_rgb(120, 120, 120)),
        Some(a) if a < 1.0 => ("●", Color32::from_rgb(50, 200, 50)),
        Some(a) if a < 5.0 => ("●", Color32::from_rgb(200, 160, 40)),
        Some(_) => ("○", Color32::from_rgb(160, 160, 160)),
    };
    let tip = match age {
        None => format!("{}: never seen", label),
        Some(a) => format!(
            "{}: {} ago, {} frames since start",
            label,
            format_duration_short(a),
            count
        ),
    };
    ui.colored_label(color, RichText::new(format!("{} {}", sym, label)))
        .on_hover_text(tip);
}

/// Render a single device dot: filled = configured at server-start,
/// hollow = absent. Hover-tip clarifies that the dot is configuration-state,
/// not live link-health.
fn render_device_dot(ui: &mut egui::Ui, label: &str, configured: bool) {
    let (sym, color, tip) = if configured {
        (
            "●",
            Color32::from_rgb(80, 140, 200),
            format!("{}: configured at server-start", label),
        )
    } else {
        (
            "○",
            Color32::from_rgb(120, 120, 120),
            format!("{}: not configured", label),
        )
    };
    ui.colored_label(color, RichText::new(format!("{} {}", sym, label)))
        .on_hover_text(tip);
}

/// Format a seconds value as "Xm Ys" (or "Ys" when under a minute).
fn format_duration_short(secs: f32) -> String {
    let total = secs.max(0.0) as u64;
    let m = total / 60;
    let s = total % 60;
    if m == 0 {
        format!("{}s", s)
    } else if m < 60 {
        format!("{}m {}s", m, s)
    } else {
        let h = m / 60;
        let m = m % 60;
        format!("{}h {}m", h, m)
    }
}
