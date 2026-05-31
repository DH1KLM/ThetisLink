# Changelog

All notable changes to ThetisLink are documented in this file. The format is
loosely based on [Keep a Changelog](https://keepachangelog.com/) and the
project follows [Semantic Versioning](https://semver.org/). Public releases
are tagged on the [`cjenschede/ThetisLink`](https://github.com/cjenschede/ThetisLink)
mirror and shipped as zipped binary bundles via that repository's Releases
page; this file is the user-facing summary so an upgrade-decision can be made
from one document.

For the protocol-level technical reference and the in-depth multi-tuner
hardware notes, see `docs-book/src/technical-reference.md` and
`docs-book/src/user-manual-en.md` (English) or
`docs-book/src/technische-referentie.md` and `docs-book/src/user-manual.md`
(Dutch).

---

## [2.0.4] — 2026-05 (bandwidth toolkit, preventive TX-inhibit, power-cap, PstRotator)

> **Backwards-compatible with 2.0.3.** Wire-protocol additive only —
> one new control ID (`DxSpotsEnabled`); older clients ignore it,
> older servers default the new behaviour to ON. Mix v2.0.3 and
> v2.0.4 freely while you roll out, but pair v2.0.4 with the
> matching Thetis-fork build to unlock the full feature-set.

### Added — Preventive RX-only TX-inhibit (Thetis-fork TL2-3 required)

A new chokepoint between ThetisLink and Thetis stops the radio from
transmitting on an antenna position marked as **RX-only**, before TX
can briefly come up. When the fork-side ThetisLink extensions are
enabled, ThetisLink drives Thetis' "Receive only" flag directly via
the new `rx_only_ex` TCI command — MOX, spacebar, hardware-PTT and
VOX are all refused at the source instead of being flipped back
reactively. The reactive ZZTX0 catch-all remains the safety floor
for stock Thetis (no fork extensions) and for any path the
preventive gate cannot reach.

- Server-side state machine handles takeover, level-maintain and
  release, including a bootstrap-stale clear so a leftover
  `RXOnly=true` from a previous session is wiped within ~1 ms after
  the cap is detected.
- The Thetis-fork `RXOnly` setter now broadcasts a TCI push-notify on
  every real transition, so external Setup → "Receive only" toggles
  are visible to ThetisLink in real time (was: only on TCI SET/GET
  echoes, which left ThetisLink with a stale cache).
- Server-side dedup on the `rx_only_ex` notification keeps the log
  clean when fork-broadcast and handler-echo arrive together.

Requires Thetis fork **PA3GHM TL2-3** or newer for the full
preventive path. Stock Thetis falls back to the reactive ZZTX0
catch-all without any user action.

### Added — Reactive RF power-cap per antenna position

Per-Amplitec-A position the server enforces a maximum forward-power
(`amplitec_max_w`) by sending the PA's own `DriveDown` button (SPE
Expert or RF2K-S) — not ZZPC, which the PA pushes back through the
TCI loop. Mode-multipliers are applied universally: SSB/CW × 1.0,
AM × 0.5, FM/DIG × 0.4. A counter remembers how many DriveDowns
were sent on the active position and restores them as DriveUps when
the user switches to a different position.

- New first-class GUI editor in the Amplitec tab (6 rows × max W
  + TX-blocked checkbox) replaces the previous file-edit workflow.
- Rate-limited to one DriveDown per second to let the PA-meter
  settle; brief CW-bursts under that interval may pass the cap
  (reactive only — preventive coverage exists on RX-only positions).
- Tuner first-config: the server UI now shows tuner slots without
  requiring an existing instance, breaking the catch-22 for new
  installs.

### Added — PstRotator UDP/XML rotor backend

Native PstRotator support alongside the existing rotctl-TCP backend.
Per-installation choice via `rotor_backend = pstrotator` in the
server config. Integer-degree AZIMUTH commands; AZ/EL replies parsed
fallible; offline-timeout marks status `false` cleanly. Host field
is a **numeric IP address** — no DNS resolution. mDNS troubleshooting
notes added to the manuals.

### Added — Editable WebSDR favorite names

Favorites in the WebSDR list now have an explicit Edit-toggle so a
rename commits on Done / loss of focus and survives reconnect.

### Added — Server-tab bandwidth monitor + DX-spots opt-out

The desktop Server tab now shows the live UDP bandwidth in both
directions:

- **Down (RX)** and **Up (TX)** in Kbit/s, updated every 500 ms.
- Click on Down to expand a per-stream breakdown (audio, spectrum,
  S-meter, DX-spots, …) refreshed every 5 s.
- A **DX spots ontvangen** checkbox lets you opt out of the DX-cluster
  spot stream on metered links. The Android client has the same
  switch in Settings.

The monitor counts UDP application-payload bytes; the operating-
system network meter typically reads 1.5–2× higher because it
includes IP/UDP/Ethernet headers. The Android DX-spots toggle
resets to ON when the app restarts (no preference persistence).

### Fixed — DX-cluster spot broadcast storm (~90 Kbit/s → ~6 Kbit/s)

The server used to re-send all cached DX-cluster spots to every
client on every equipment-tick (5 Hz). With ~100 spots in cache this
consumed ~90 Kbit/s steady-state on each client. The broadcast now
sends only new spots per tick and triggers a full age-refresh every
10 s — about 15× less data without a user-visible change.

### Fixed — Server log spam

Two periodic log sources became state-change-driven:

- `PowerCap state` only logs when `(pos, mode, pa_in_operate, cap)`
  transitions — used to fire every 2 s regardless. PA-meter fluctuations
  are intentionally excluded from the snapshot so they don't reintroduce
  the spam.
- `DX Cluster` reconnect now emits one line per failure plus one
  line on recovery (`reconnected after N failed attempts`) instead of
  the previous three lines per backoff cycle.

### Notes for upgraders

- A v2.0.3 client connects to a v2.0.4 server without problems; the
  new `DxSpotsEnabled` control is simply unused, default = ON.
- A v2.0.4 client connects to a v2.0.3 server without problems; the
  opt-out toggle is harmless (server ignores the unknown control)
  but you cannot turn off the spot stream on the older server.
- The new preventive TX-inhibit only activates when paired with a
  **Thetis fork build PA3GHM TL2-3 or newer**. Stock Thetis remains
  fully supported via the reactive ZZTX0 fallback that already
  existed in v2.0.3.

---

## [2.0.3] — 2026-05 (multi-tuner + wire-protocol breaking change)

> **Breaking change — wire-protocol version bumped from 2 → 3.**
> A v2.0.3 server and a v2.0.2 client (or vice versa) are not compatible:
> the S-meter payload layout was rearranged to support multi-source
> subscriptions (Sig peak-hold, Avg true-mean, MaxBin) and an S9-frequency
> band-shift. Mismatched pairs are detected in the handshake and the user
> sees a localised `ProtocolVersionMismatch` modal ("Server is too old" /
> "Client is too old") instead of garbled audio or a silent connect failure.
> Upgrade server, desktop client and Android client together.

### Added — multi-tuner runtime via Adafruit MCP2221A USB-HID

- Up to **two physical StockCorner JC-4s / JC-3s tuners in parallel**, each
  driven through its own MCP2221A breakout (replaces the v2.0.2 serial-port
  RTS/CTS flow). JC-4s and JC-3s share the same control protocol; the model
  alias is cosmetic.
- **Per-tuner status panel rows** with: connection state, MCP serial
  dropdown, Amplitec-A position binding, live yellow-wire voltage,
  threshold slider (0.5 V – 4.5 V, default 2.25 V), hysteresis slider
  (0.1 V – 2.0 V, default 0.50 V), and the derived `active < … V` /
  `idle > … V` edge display. An amber **⚠ clamped** warning appears when
  the slider combination falls outside the physically reachable yellow
  range so the user sees that the configuration would never trigger.
- **USB board scan + "Program serial"** UI: identify anonymous boards by
  HID path, give each one a unique serial that survives a USB-replug.
- **USB auto-reconnect** (5 s retry interval) for tuner bridges that drop
  the link after first connecting — no server restart needed to recover
  from a cable replug or hub reset.
- **Collapsible MCP2221A section** at the bottom of the status panel with
  its open/closed state persisted across server restarts
  (`mcp2221_section_expanded` config key).

### Added — S-meter and TCI sensor layer

- Multi-source S-meter subscription via the `rx_channel_sensors_ex` TCI
  payload: peak-hold ("Sig"), true-mean ("Avg"), and the single highest
  FFT-bin in the passband ("MaxBin") are all cached server-side; clients
  pick the source they want via `SmeterSource`.
- **S9-frequency band shift** (HF vs VHF/UHF S-meter scale) honours the
  Thetis-fork-provided crossover frequency (`s9_frequency_ex`); falls
  back to 50 MHz against stock Thetis.
- FWD-power continues to update during TX with Sig/MaxBin subscription
  active (previous bundle showed zero forward power when the client
  switched away from the Avg source).

### Added — CTUN, MIDI, Spectrum

- **CTUN coupled-recenter mirror**: server now syncs the second RX
  spectrum so rapid VFO-A tuning does not leave RX2 lagging.
- **MIDI client-side VFO coalesce** + auto-recenter ownership handshake
  with the Thetis fork: extreme MIDI wheel input no longer fills the
  VFO queue, and the fork-side smooth-scroll guard now also works when
  no ThetisLink server is connected.
- Connect-time RX1/RX2 spectrum **balancing** so both RX paths come up
  at roughly the same moment; Auto-FFT retuned to ~25 FPS.

### Added — Persistence and UI polish

- PA `active_pa` choice and per-PA pre-Operate drive snapshots survive
  a non-graceful shutdown (process kill / power loss) without waiting
  for the next `start_server()` write.
- RF2K-S drive restore guard: no `ZZPC000;` is sent if the snapshot
  is missing.
- Status-panel **protocol version-mismatch** banner: previously the
  mismatch was silent; now there is a visible row when a v2.0.2 client
  contacts the v2.0.3 server (or vice versa).
- Scrollable Radio tab, resizable spectrum split, persisted pop-out
  window geometry.

### Changed

- Tuner removal: the v2.0.2 `assume_tuned` checkbox, `TUNER_DONE_ASSUMED`
  state (5) and the 500 ms assume-deadline pad were retired now that
  feedback-driven tune-detection works reliably in production. The
  client/Android paths that still recognise state 5 are harmless dead
  paths and will be removed in v2.0.4.
- Voltage divider on the yellow tune-status wire: both R1 and R2
  moved from 10 kΩ to **1 MΩ** to reduce loading on the JC-Control
  LED circuit. Ratio (1:1, ×2 in voltage) and the threshold defaults are
  unchanged. The full wiring schema is documented separately and
  available on request.

### Fixed

- **Config RMW race** in the status-panel write paths: per-tuner MCP
  serial, Amplitec-pos, threshold and hysteresis edits now go through
  `config::modify_config(|c| …)` so the load/mutate/save sequence is
  atomic under `CONFIG_LOCK` — closing the same race that was fixed for
  the RF2K drive snapshot earlier in the v2.0.3 cycle but had been
  reintroduced by the new tuner UI.
- **ADC dedup** in the tuner thread: the bridge `snapshot()` rate-limits
  USB ADC polls to 100 ms while the tuner thread runs the active/idle
  edge loops at 25 ms. The thread now checks the
  `DebugSnapshot.last_adc_at` timestamp and only counts a consecutive
  edge when the timestamp has advanced — defeating the "count the same
  cached sample twice" failure mode that defeated noise rejection.
- **Threshold/hysteresis edge-clamp**: slider combinations like
  threshold 0.5 V + hysteresis 2.0 V used to produce an unreachable
  active edge at −0.5 V. The bridge now clamps the computed edges to
  the physically reachable range `[0, ADC_VREF × divider]` and exposes
  `edges_clamped: bool` so the UI can show an amber warning instead of
  silently locking the tuner.

### Compliance

- `vendor/mcp2221-hal/` v0.1.0 (Copyright © 2025 Rob Wells, MIT branch
  of the dual-licensed `MIT OR Apache-2.0` source) added as a vendored
  Rust crate; attribution added to `NOTICE.md` and the full MIT license
  text is reproduced in `compliance/THIRD-PARTY-LICENSES.html` and the
  SPDX SBOM at `compliance/sbom.spdx.json`. The Apache-2.0 alternative
  is deliberately not used because Apache-2.0's patent-grant clauses
  are not compatible with the `GPL-2.0-or-later` licence ThetisLink
  itself is distributed under.
- All `compliance/*` artefacts regenerated for the v2.0.3 binary set.

---

## [2.0.2] — 2026-05 (log-spam hotfix)

- Server-side `DiversityPhaseEx`, `DiversityGainEx` and
  `DiversityGainMultiEx` TCI notifications now log INFO only on a real
  value change. Thetis pushes these at every diversity tick (~10–20 Hz)
  which previously filled the server log at hundreds of thousands of
  lines per session.
- Functional behaviour and wire protocol unchanged (`VERSION = 2`) —
  fully interoperable with v2.0.0 and v2.0.1.

## [2.0.1] — 2026-05 (connect-experience release)

- First-run 4-step setup wizard (Find server → Password → 2FA →
  Connected).
- mDNS local-network discovery — clients auto-find servers on the same
  WiFi/LAN.
- Nine differentiated connect states with platform-aware NL/EN hints
  including a smart `TciUnreachable` hint that knows whether Thetis is
  running, starting up, or fully stopped.
- Server status panel: bind address, TCI status, active clients with
  RTT / loss / jitter, audio routing chips, recent connect attempts.
- Server-side RX2 audio-filter fix (no more phantom CH2 stream when RX2
  is off).
- "Restart setup wizard" button.
- Wire protocol unchanged (`VERSION = 2`) — fully interoperable with
  v2.0.0.

## [2.0.0] — 2026-04 (TL2 release)

- Yaesu FT-991A auto-DFM PTT toggle (FM ↔ DATA-FM with memory restore).
- Server-side CTUN auto-recenter (Thetis-fork `auto_recenter_ex`).
- Live diversity null-circle broadcast (Smart / Ultra).
- Filter-preset push (F1..VAR2/NONE), per-RX DDC sample rate
  (48..1536 kHz), `tci_caps_ex` capability broadcast.
- DX cluster click-to-tune, SWR display in TX meter.
- CW keyer + macros over TCI.
- Single-TCI-only architecture — separate CAT connection retired.
- **Wire-protocol VERSION bumped from 1 → 2.**

## [1.0.0] — 2026-03

- First public release on [`cjenschede/ThetisLink`](https://github.com/cjenschede/ThetisLink).

## [0.5.0]

- Yaesu FT-991A support, Bluetooth headset (Android), diversity-receive
  fix, TCI control elements, RF2K-S reset, PTT modes, DX cluster.

## [0.4.9]

- Wideband Opus TX, device-switch fix.

## [0.4.2]

- Configurable FFT size, dynamic spectrum bins, Android power-button fix.

## [0.4.1]

- WebSDR / KiwiSDR integration, frequency sync, TX-spectrum auto-override.

## [0.4.0]

- TCI WebSocket, waterfall click-to-tune (Android).

## [0.3.2]

- MIDI controller support, PTT toggle with LED, mic AGC.

## [0.3.1]

- Band memory, FM filter fix, macOS client.

## [0.3.0]

- Full RX2 / VFO-B support, DDC spectrum + waterfall.
