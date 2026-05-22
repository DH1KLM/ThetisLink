# NOTICE

ThetisLink is a Rust application that interoperates with, and for the Thetis
SDR fork maintained in `cjenschede/Thetis` directly continues, the Thetis
software developed within the OpenHPSDR ecosystem. It is distributed under
the GNU General Public License v2.0-or-later (see `LICENSE`).

See `ATTRIBUTION.md` for the full list of contributors to the upstream
Thetis lineage whose work this project builds upon, and for information
about the dual-licensing statement applicable to contributions by Richard
Samphire (MW0LGE).

Contact for commercial licensing: see `LICENSE-COMMERCIAL.md`.

## Third-party components

ThetisLink vendors and statically links the following third-party Rust crate
as source under `vendor/`, in addition to the crates resolved from
[crates.io](https://crates.io/) at build time. Full per-crate license texts
and SPDX identifiers for every transitive dependency are in
`compliance/THIRD-PARTY-LICENSES.html` and `compliance/sbom.spdx.json`.

- **`mcp2221-hal` v0.1.0** — Copyright © 2025 Rob Wells, dual-licensed
  `MIT OR Apache-2.0`. ThetisLink uses this crate under the **MIT** terms
  (the GPL-2.0-or-later license under which ThetisLink itself is
  distributed is compatible with MIT; the Apache-2.0 alternative is not
  used because Apache-2.0's patent-grant clauses are not compatible with
  GPL-2.0-only). Upstream source:
  <https://github.com/robjwells/mcp2221-hal/>. See
  `vendor/mcp2221-hal/LICENSE-MIT` for the full MIT license text.
