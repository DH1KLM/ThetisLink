# ThetisLink

> **Current release: [v2.0.4](https://github.com/cjenschede/ThetisLink/releases/tag/v2.0.4)** —
> preventive RX-only TX-inhibit via the new Thetis-fork `rx_only_ex` command
> (MOX, spacebar, hardware-PTT and VOX refused at the source on an RX-only
> Amplitec position), reactive RF power-cap per antenna position with PA-native
> DriveDown (SPE + RF2K-S), PstRotator UDP/XML rotor backend, server-tab
> bandwidth monitor with per-stream breakdown, per-client DX-spots opt-out
> (~90 Kbit/s broadcast storm fixed → ~6 Kbit/s steady-state).
> **Backwards-compatible with v2.0.3** — wire-protocol additive only;
> pair with **Thetis fork PA3GHM TL2-3** for the full feature-set, stock
> Thetis falls back gracefully to the reactive `ZZTX0` catch-all that
> already existed. Download `ThetisLink-2.0.4.zip` from the
> [Releases page](https://github.com/cjenschede/ThetisLink/releases) — the ZIP
> contains both Windows binaries, the Android APK, all six PDF manuals,
> `LICENSE` and `SHA256SUMS.txt`. SBOM and third-party license artefacts are
> attached to the same release as separate download assets.

Remote control for ANAN 7000DLE SDR with Thetis. Audio, spectrum, PTT and full
radio control over the network via TCI WebSocket.

## Components

- **ThetisLink Server** — runs on the Thetis PC (Windows), controls the radio via TCI
- **ThetisLink Client** — desktop client (Windows) with spectrum, waterfall and full control
- **ThetisLink Android** — mobile client app

## Features

- Real-time bidirectional audio (Opus codec, minimal latency)
- Spectrum and waterfall display (up to 1536 kHz with the PA3GHM Thetis fork)
- Full RX2/VFO-B support with diversity reception
- External device control: Amplitec 6/2, two StockCorner JC-4s/JC-3s tuners in parallel (MCP2221A USB-HID), SPE Expert 1.3K-FA, RF2K-S, UltraBeam RCU-06, EA7HG Visual Rotor
- Yaesu FT-991A as second radio (CAT + USB audio)
- MIDI controller support (desktop + Android)
- DX Cluster with spectrum overlay
- Mandatory password authentication (HMAC-SHA256) with optional TOTP 2FA
- Smart and Ultra diversity auto-null algorithms

## Documentation

Included with each release:

- `Installatie.md` / `Installation.md` — installation guide (Dutch / English)
- `User-Manual.md` / `User-Manual-EN.md` — user manual (Dutch / English)
- `Technische-Referentie.md` / `Technical-Reference.md` — technical reference

## Thetis compatibility

ThetisLink talks to the radio through Thetis. It targets **Thetis v2.10.3.15**
(the latest official release by ramdor) and works with stock Thetis out of the
box. Optionally use the [PA3GHM Thetis fork](https://github.com/cjenschede/Thetis/tree/thetislink-tl2)
(branch `thetislink-tl2`) for the additional `_ex` TCI extensions used by
ThetisLink v2.0.4 (capability broadcast, per-RX filter preset, diversity
control suite, server-side DDC recenter, relaxed IQ-stream rate cap). All
extensions are gated behind the **ThetisLink extensions** checkbox in Setup
> Network > IQ Stream; with the checkbox unchecked the fork behaves like
stock Thetis.

The Thetis fork is maintained separately from this repository. Its per-file
source headers grant the GNU General Public License "version 2 or (at your
option) any later version", corresponding to the SPDX identifier
`GPL-2.0-or-later`. For authoritative details, see that repository's own
`LICENSE`, `LICENSE-DUAL-LICENSING`, and source-file headers.

## License and attribution

ThetisLink is distributed under **GNU General Public License v2.0-or-later**.
See:

- [`LICENSE`](LICENSE) — canonical GPLv2 text
- [`NOTICE.md`](NOTICE.md) — top-level notice
- [`ATTRIBUTION.md`](ATTRIBUTION.md) — Thetis-lineage contributor attribution
  and scope of this project's derivative relationship
- [`LICENSE-COMMERCIAL.md`](LICENSE-COMMERCIAL.md) — commercial licensing
  enquiries (the GPL version is appropriate for amateur radio and personal use)

ThetisLink builds upon the work of the OpenHPSDR Thetis lineage. We acknowledge
all upstream contributors — see `ATTRIBUTION.md` for the full list.

## Support

If you find ThetisLink useful, consider buying me a coffee:

[Donate via PayPal](https://paypal.me/PA3GHM)

73 de PA3GHM
