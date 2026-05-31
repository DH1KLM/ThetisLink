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
                    | crate::session::ConnectOutcome::WrongTotp
                    | crate::session::ConnectOutcome::ProtocolVersionMismatch { .. } => {
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

    // ── 7. MCP2221A tuner bridges — collapsible so the owner can hide the
    // per-tuner threshold sliders and board-scan tools during normal use.
    // Open/closed state is persisted in the config so it survives a restart.
    ui.separator();
    let current_expanded = crate::config::load().mcp2221_section_expanded;
    let resp = egui::CollapsingHeader::new(
            RichText::new("MCP2221A tuner bridges").strong(),
        )
        .open(Some(current_expanded))
        .show(ui, |ui| {
            // 7a — per-tuner rows. We renderen ALTIJD 2 slots (de hardware
            // ondersteunt maximaal 2 MCP2221A bridges); een nog-niet-
            // geconfigureerd slot krijgt een lichtere config-row met alleen
            // de "MCP serial" en "Amplitec pos" dropdowns. Zonder dit kan de
            // owner bij een schone config de eerste koppeling niet maken
            // (catch-22: geen mcp_serial → geen running TunerInstance →
            // geen UI om mcp_serial te kiezen).
            let active = shared.tuners_slot.get();
            for slot in 0..2usize {
                let inst = active.and_then(|t| {
                    t.instances().iter().find(|i| i.slot_index() == slot)
                });
                match inst {
                    Some(inst) => render_tuner_detector_row(ui, inst),
                    None => render_tuner_disabled_row(ui, slot),
                }
            }
            // 7b — board scan: list all MCP2221A devices on the USB bus so
            // the owner can figure out which USB serial belongs to which
            // physical tuner. Result cached statically until the next Scan
            // click so we don't hammer the HID enumerator on every repaint.
            ui.separator();
            render_board_scan_section(ui);
        });
    // Detect a header click and toggle the persisted state. We use
    // `.open(Some(...))` so egui ignores clicks itself — we drive the
    // open/closed state from config and persist on every flip.
    if resp.header_response.clicked() {
        crate::config::save_mcp2221_section_expanded(!current_expanded);
    }
}

fn render_tuner_detector_row(
    ui: &mut egui::Ui,
    inst: &std::sync::Arc<crate::tuner::TunerInstance>,
) {
    let bridge = inst.bridge();
    let snap = bridge.snapshot();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        // Header: tuner label + connection status + model dropdown
        ui.horizontal(|ui| {
            ui.label(RichText::new(inst.label()).strong());
            match &snap.status {
                crate::mcp2221_debug::Status::Connected => {
                    ui.colored_label(Color32::from_rgb(50, 200, 50), "Connected");
                }
                crate::mcp2221_debug::Status::NotInitialized => {
                    ui.colored_label(Color32::from_rgb(160, 160, 160), "Not connected");
                }
                crate::mcp2221_debug::Status::Error(e) => {
                    ui.colored_label(
                        Color32::from_rgb(220, 60, 60),
                        format!("Error: {}", e),
                    );
                }
            }
        });
        // MCP serial selector — pick which physical Adafruit board (by USB
        // serial name, as the owner programmed it) is assigned to this tuner
        // slot. Options come from the latest scan cache; the current value
        // is always present even if the board is currently unplugged.
        // Changing the selection writes the new serial to config and
        // auto-restarts so the bridge re-opens on the chosen board.
        ui.horizontal(|ui| {
            ui.label("MCP serial:");
            let slot = inst.slot_index();
            let current_serial: String = crate::config::load()
                .tuners
                .get(slot)
                .map(|c| c.mcp_serial.clone())
                .unwrap_or_default();
            let mut chosen = current_serial.clone();
            let scan_results = cached_scan_serials();
            egui::ComboBox::from_id_salt(format!("tuner_mcp_serial_{}", slot))
                .selected_text(if chosen.is_empty() {
                    "(first available)".to_string()
                } else {
                    chosen.clone()
                })
                .show_ui(ui, |ui| {
                    // Build a deduplicated list: detected boards' serials
                    // PLUS the currently-configured one (so an unplugged
                    // assignment is still selectable / visible).
                    let mut options: Vec<String> = scan_results
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect();
                    if !current_serial.is_empty()
                        && !options.iter().any(|s| s == &current_serial)
                    {
                        options.push(current_serial.clone());
                    }
                    options.sort();
                    options.dedup();
                    for opt in &options {
                        ui.selectable_value(&mut chosen, opt.clone(), opt);
                    }
                });
            if chosen != current_serial {
                let chosen_for_log = chosen.clone();
                let mut applied = false;
                crate::config::modify_config(|config| {
                    if slot < config.tuners.len() {
                        config.tuners[slot].mcp_serial = chosen.clone();
                        config.tuners[slot].model = infer_model_from_serial(&chosen);
                        config.tuners[slot].enabled = true;
                        applied = true;
                    }
                });
                if applied {
                    log::info!(
                        "Tuner {} MCP serial changed to \"{}\" — auto-restart",
                        slot + 1,
                        chosen_for_log,
                    );
                    restart_server();
                }
            }
        });
        // Amplitec-A position picker — drives the Tune-button routing in
        // network.rs. Options come from the live config so the labels stay
        // in sync with whatever the owner has set per antenna position.
        ui.horizontal(|ui| {
            ui.label("Amplitec pos:");
            let slot = inst.slot_index();
            let live_config = crate::config::load();
            let current_pos: Option<u8> = live_config
                .tuners
                .get(slot)
                .and_then(|c| c.amplitec_pos);
            let labels = live_config.amplitec_labels.clone();
            let mut chosen: Option<u8> = current_pos;
            let selected_text = match current_pos {
                None => "(none)".to_string(),
                Some(p) if (1..=6).contains(&p) => {
                    let lbl = &labels[(p - 1) as usize];
                    if lbl.is_empty() {
                        format!("{}", p)
                    } else {
                        format!("{}: {}", p, lbl)
                    }
                }
                Some(p) => format!("{} (out of range)", p),
            };
            egui::ComboBox::from_id_salt(format!("tuner_amplitec_pos_{}", slot))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "(none)");
                    for p in 1u8..=6 {
                        let lbl = &labels[(p - 1) as usize];
                        let label_text = if lbl.is_empty() {
                            format!("{}", p)
                        } else {
                            format!("{}: {}", p, lbl)
                        };
                        ui.selectable_value(&mut chosen, Some(p), label_text);
                    }
                });
            if chosen != current_pos {
                let mut applied = false;
                crate::config::modify_config(|config| {
                    if slot < config.tuners.len() {
                        config.tuners[slot].amplitec_pos = chosen;
                        applied = true;
                    }
                });
                if applied {
                    log::info!(
                        "Tuner {} amplitec_pos changed to {:?} — auto-restart",
                        slot + 1,
                        chosen,
                    );
                    restart_server();
                }
            }
        });
        // Live yellow-wire voltage — single value, since raw and pre-divider
        // pin voltage add nothing for the owner who reads the schema in
        // post-divider volts.
        ui.horizontal(|ui| {
            ui.label("Live:");
            match snap.last_yellow_v {
                Some(yellow_v) => {
                    ui.colored_label(
                        ui.visuals().text_color(),
                        RichText::new(format!("yellow {:.2} V", yellow_v)).monospace(),
                    );
                }
                None => {
                    ui.colored_label(Color32::from_rgb(160, 160, 160), "(no sample)");
                }
            }
        });
        // Threshold slider — switch level on the yellow wire (V). Changes
        // apply to the live bridge AND persist to config so they survive a
        // restart.
        ui.horizontal(|ui| {
            let slot = inst.slot_index();
            ui.label("Threshold:");
            let mut v = snap.threshold_v;
            if ui
                .add(egui::Slider::new(&mut v, 0.5..=4.5).suffix(" V"))
                .changed()
            {
                bridge.set_threshold_v(v);
                persist_threshold_v(slot, v);
            }
        });
        // Hysteresis slider — width of the dead-band around the threshold (V).
        ui.horizontal(|ui| {
            let slot = inst.slot_index();
            ui.label("Hysteresis:");
            let mut v = snap.hysteresis_v;
            if ui
                .add(egui::Slider::new(&mut v, 0.1..=2.0).suffix(" V"))
                .changed()
            {
                bridge.set_hysteresis_v(v);
                persist_hysteresis_v(slot, v);
            }
        });
        // Computed edges, in V — what the bridge will compare each sample to.
        // When threshold ± hyst/2 falls outside the physically-reachable
        // yellow range (0..ADC_VREF*divider), the edges are clamped and an
        // amber warning is shown so the owner sees the slider combo is
        // degenerate (would never actually trigger a tune on hardware).
        ui.horizontal(|ui| {
            ui.label("Edges:");
            let edges_text = format!(
                "active < {:.2} V   idle > {:.2} V",
                snap.threshold_active_v, snap.threshold_idle_v
            );
            if snap.edges_clamped {
                ui.colored_label(
                    Color32::from_rgb(220, 160, 40),
                    RichText::new(format!("{}   ⚠ clamped", edges_text)).monospace(),
                )
                .on_hover_text(
                    "Threshold + hysteresis falls outside the reachable yellow range. \
                     Lower the hysteresis or move the threshold further from the boundary.",
                );
            } else {
                ui.colored_label(
                    ui.visuals().text_color(),
                    RichText::new(edges_text).monospace(),
                );
            }
        });
    });
    // Keep the Live voltage ticking ~10×/s even when the user is not
    // interacting. snapshot() rate-limits the actual USB poll to 100 ms so
    // this is cheap.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
}

/// Lichte config-row voor een tuner-slot dat nog niet enabled is. Toont
/// alleen de MCP-serial en Amplitec-pos dropdowns; de voltage/threshold
/// sliders zijn weggelaten omdat er geen actieve bridge is om uit te
/// lezen of te configureren. Zodra de owner een MCP serial selecteert
/// wordt de tuner enabled gezet en de server geherstart (zelfde
/// auto-restart pad als de full row).
fn render_tuner_disabled_row(ui: &mut egui::Ui, slot: usize) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Tuner {}", slot + 1)).strong());
            ui.colored_label(
                Color32::from_rgb(160, 160, 160),
                "Disabled \u{2014} select an MCP serial to enable",
            );
        });
        // MCP serial dropdown — selecteren enabled het slot + auto-restart.
        ui.horizontal(|ui| {
            ui.label("MCP serial:");
            let current_serial: String = crate::config::load()
                .tuners
                .get(slot)
                .map(|c| c.mcp_serial.clone())
                .unwrap_or_default();
            let mut chosen = current_serial.clone();
            let scan_results = cached_scan_serials();
            egui::ComboBox::from_id_salt(format!("tuner_mcp_serial_disabled_{}", slot))
                .selected_text(if chosen.is_empty() {
                    "(select board)".to_string()
                } else {
                    chosen.clone()
                })
                .show_ui(ui, |ui| {
                    let mut options: Vec<String> = scan_results
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect();
                    if !current_serial.is_empty()
                        && !options.iter().any(|s| s == &current_serial)
                    {
                        options.push(current_serial.clone());
                    }
                    options.sort();
                    options.dedup();
                    if options.is_empty() {
                        ui.colored_label(
                            Color32::from_rgb(180, 180, 180),
                            "(no boards detected \u{2014} run Scan below)",
                        );
                    } else {
                        for opt in &options {
                            ui.selectable_value(&mut chosen, opt.clone(), opt);
                        }
                    }
                });
            if chosen != current_serial && !chosen.is_empty() {
                let chosen_for_log = chosen.clone();
                let mut applied = false;
                crate::config::modify_config(|config| {
                    if slot < config.tuners.len() {
                        config.tuners[slot].mcp_serial = chosen.clone();
                        config.tuners[slot].model = infer_model_from_serial(&chosen);
                        config.tuners[slot].enabled = true;
                        applied = true;
                    }
                });
                if applied {
                    log::info!(
                        "Tuner {} MCP serial set to \"{}\" \u{2014} auto-restart",
                        slot + 1,
                        chosen_for_log,
                    );
                    restart_server();
                }
            }
        });
        // Amplitec pos dropdown — kan al gezet worden vóór het slot enabled
        // is, maar levert weinig op zonder een actieve tuner; we tonen 'm
        // toch zodat de owner alles in één veld-sessie kan instellen.
        ui.horizontal(|ui| {
            ui.label("Amplitec pos:");
            let live_config = crate::config::load();
            let current_pos: Option<u8> = live_config
                .tuners
                .get(slot)
                .and_then(|c| c.amplitec_pos);
            let labels = live_config.amplitec_labels.clone();
            let mut chosen: Option<u8> = current_pos;
            let selected_text = match current_pos {
                None => "(none)".to_string(),
                Some(p) if (1..=6).contains(&p) => {
                    let lbl = &labels[(p - 1) as usize];
                    if lbl.is_empty() {
                        format!("{}", p)
                    } else {
                        format!("{}: {}", p, lbl)
                    }
                }
                Some(p) => format!("{} (out of range)", p),
            };
            egui::ComboBox::from_id_salt(format!("tuner_amplitec_pos_disabled_{}", slot))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "(none)");
                    for p in 1u8..=6 {
                        let lbl = &labels[(p - 1) as usize];
                        let label_text = if lbl.is_empty() {
                            format!("{}", p)
                        } else {
                            format!("{}: {}", p, lbl)
                        };
                        ui.selectable_value(&mut chosen, Some(p), label_text);
                    }
                });
            if chosen != current_pos {
                let mut applied = false;
                crate::config::modify_config(|config| {
                    if slot < config.tuners.len() {
                        config.tuners[slot].amplitec_pos = chosen;
                        applied = true;
                    }
                });
                if applied {
                    log::info!(
                        "Tuner {} amplitec_pos set to {:?} \u{2014} auto-restart",
                        slot + 1,
                        chosen,
                    );
                    restart_server();
                }
            }
        });
    });
}

/// Per-board scratch state for the "Program serial" text input.
/// Keyed by USB HID path so two anonymous boards do not collide.
fn board_serial_edit_state(path: &str) -> std::sync::Arc<std::sync::Mutex<String>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static MAP: OnceLock<Mutex<HashMap<String, Arc<Mutex<String>>>>> = OnceLock::new();
    let map = MAP.get_or_init(|| Mutex::new(HashMap::new()));
    let mut m = map.lock().unwrap();
    m.entry(path.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(String::new())))
        .clone()
}

/// Per-board result of the most recent `program_serial_at_path` call.
fn board_program_result_state(
    path: &str,
) -> std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static MAP: OnceLock<Mutex<HashMap<String, Arc<Mutex<Option<Result<(), String>>>>>>> =
        OnceLock::new();
    let map = MAP.get_or_init(|| Mutex::new(HashMap::new()));
    let mut m = map.lock().unwrap();
    m.entry(path.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone()
}

/// Persist a tuner slot's switch threshold (V). Saves silently (no
/// auto-restart): `set_threshold_v` already updates the live bridge so the
/// new value is effective immediately. Uses `modify_config` so the load /
/// mutate / save sequence is atomic under `CONFIG_LOCK` — preventing the
/// build-58 RMW race from being reintroduced via the UI slider.
fn persist_threshold_v(slot: usize, v: f32) {
    crate::config::modify_config(|config| {
        if slot < config.tuners.len() {
            config.tuners[slot].threshold_v = v;
        }
    });
}

/// Persist a tuner slot's hysteresis (V). Same locking story as
/// `persist_threshold_v`.
fn persist_hysteresis_v(slot: usize, v: f32) {
    crate::config::modify_config(|config| {
        if slot < config.tuners.len() {
            config.tuners[slot].hysteresis_v = v;
        }
    });
}

/// Heuristic: pick the cosmetic [`TunerModel`] from a freshly-programmed USB
/// serial name. Anything containing `3s` (case-insensitive) maps to JC-3s,
/// everything else defaults to JC-4s. Owners who use entirely different
/// naming conventions just override via the bridge-row model dropdown.
fn infer_model_from_serial(serial: &str) -> crate::config::TunerModel {
    if serial.to_lowercase().contains("3s") {
        crate::config::TunerModel::Jc3s
    } else {
        crate::config::TunerModel::Jc4s
    }
}

/// Self-restart the server: spawn a fresh copy of the running executable
/// with the same CLI args, then `std::process::exit(0)` so the new copy can
/// bind the UDP socket. Used by tuner-config UI actions (model dropdown,
/// "Use for Tuner N" buttons) so the owner sees the change take effect
/// without remembering to manually stop+start.
///
/// Logs the requested restart so the new instance's log starts clean — the
/// log file is opened with `truncate=true` by `GuiLogger`.
fn restart_server() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log::error!("Auto-restart: cannot read current_exe(): {}", e);
            return;
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    log::info!("Auto-restart: relaunching {:?} (args: {:?})", exe, args);

    // Build the command with explicit null stdio + (on Windows) detached
    // process flags. Without this the spawn fails with ERROR_NOT_SUPPORTED
    // (os error 50) when the parent is a GUI-subsystem binary whose stdio
    // handles are NULL: CreateProcess refuses to clone them into the child.
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS (0x00000008) — the new process gets its own
        // console handle group, detached from ours. CREATE_NEW_PROCESS_GROUP
        // (0x00000200) further isolates Ctrl-C delivery. Combined they make
        // the child fully independent so this process can exit immediately.
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    match cmd.spawn() {
        Ok(child) => {
            log::info!("Auto-restart: spawned PID {}, exiting", child.id());
            // Hard exit so the old UDP socket / MCP2221A handles release
            // before the new process tries to bind / open them.
            std::process::exit(0);
        }
        Err(e) => {
            log::error!("Auto-restart: spawn failed: {}", e);
        }
    }
}

/// Shared scan cache used by both the "Detected boards" section and the
/// per-tuner MCP-serial dropdowns. Populated by the Scan button.
fn scan_cache(
) -> &'static std::sync::Mutex<Option<Result<Vec<crate::mcp2221_scan::BoardInfo>, String>>> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<
        Mutex<Option<Result<Vec<crate::mcp2221_scan::BoardInfo>, String>>>,
    > = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Snapshot of every non-empty serial currently in the scan cache. Used by
/// the per-tuner serial dropdown so the owner can pick from physically
/// present boards. Returns an empty Vec when the owner hasn't clicked Scan
/// yet — the dropdown then only shows the currently-configured serial.
fn cached_scan_serials() -> Vec<String> {
    let cache = scan_cache().lock().unwrap();
    match cache.as_ref() {
        Some(Ok(list)) => list
            .iter()
            .map(|b| b.serial_number.clone())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn render_board_scan_section(ui: &mut egui::Ui) {
    let cache = scan_cache();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Detected MCP2221A boards:").strong());
        if ui.button("Scan").clicked() {
            let result = crate::mcp2221_scan::list_boards().map_err(|e| format!("{:?}", e));
            *cache.lock().unwrap() = Some(result);
        }
    });
    let cached = cache.lock().unwrap();
    match cached.as_ref() {
        None => {
            ui.colored_label(
                Color32::from_rgb(160, 160, 160),
                "  (click Scan to enumerate)",
            );
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.colored_label(
                Color32::from_rgb(220, 160, 40),
                "  (none detected — check USB cable)",
            );
        }
        Some(Ok(list)) => {
            for b in list {
                // Vertical per-board block so the TextEdit + button never get
                // clipped off the right edge of the panel in narrow windows.
                // Each board carries a single "Program serial" action — both
                // for anonymous boards (initial naming) and named boards
                // (rename). Tuner-slot assignment happens via the dropdown
                // inside each tuner-bridge frame above, not here.
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("• {}", b.label())).monospace(),
                    );
                    ui.label(
                        RichText::new(format!("path: {}", b.path))
                            .monospace()
                            .small()
                            .weak(),
                    );
                    ui.horizontal(|ui| {
                        let serial_present = !b.serial_number.is_empty();
                        ui.label(if serial_present {
                            "Rename serial:"
                        } else {
                            "Set serial:"
                        });
                        let text = board_serial_edit_state(&b.path);
                        let mut guard = text.lock().unwrap();
                        // Pre-fill the text input with the current serial so
                        // the owner can edit-in-place when renaming.
                        if guard.is_empty() && serial_present {
                            *guard = b.serial_number.clone();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut *guard)
                                .desired_width(180.0)
                                .hint_text("e.g. JC-4s loop"),
                        );
                        if ui.button("Program serial").clicked() {
                            let new_serial = guard.trim().to_string();
                            let path = b.path.clone();
                            drop(guard);
                            log::info!(
                                "Program serial: path=\"{}\" new_serial=\"{}\"",
                                path, new_serial
                            );
                            let res = if new_serial.is_empty() {
                                Err("serial cannot be empty".to_string())
                            } else {
                                crate::mcp2221_scan::program_serial_at_path(
                                    &path,
                                    &new_serial,
                                )
                                .map_err(|e| format!("{:?}", e))
                            };
                            match &res {
                                Ok(()) => log::info!(
                                    "Program serial OK: path=\"{}\"",
                                    path
                                ),
                                Err(msg) => log::warn!(
                                    "Program serial FAIL: path=\"{}\" err={}",
                                    path, msg
                                ),
                            }
                            *board_program_result_state(&b.path).lock().unwrap() =
                                Some(res);
                        }
                    });
                    // Result line for the most recent Program-serial attempt.
                    if let Some(res) = board_program_result_state(&b.path)
                        .lock()
                        .unwrap()
                        .as_ref()
                    {
                        match res {
                            Ok(()) => {
                                ui.colored_label(
                                    Color32::from_rgb(50, 200, 50),
                                    "written; click Scan to refresh",
                                );
                            }
                            Err(msg) => {
                                ui.colored_label(
                                    Color32::from_rgb(220, 60, 60),
                                    format!("error: {}", msg),
                                );
                            }
                        }
                    }
                });
            }
            ui.colored_label(
                Color32::from_rgb(160, 160, 160),
                "  (assign a board to a tuner via the MCP-serial dropdown in each Tuner-bridge frame above)",
            );
        }
        Some(Err(e)) => {
            ui.colored_label(
                Color32::from_rgb(220, 60, 60),
                format!("  scan failed: {}", e),
            );
        }
    }
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
