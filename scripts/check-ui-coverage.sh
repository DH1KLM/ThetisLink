#!/usr/bin/env sh
# UI coverage-gate: verifieer dat de geregistreerde control-sites overeenkomen
# met de hand-onderhouden baseline in `scripts/ui-coverage-expected.json`.
#
# Voor achtergrond: zie `scripts/README.md` en
# `sdr-remote-client/src/ui/controls/coverage.rs`.
#
# Draaien:
#   1. Bouw + start de client minstens één keer met
#      `cargo build --features ui-coverage` (of debug) en loop door alle
#      render-paden (Tab::Radio + RX1/RX2 popouts + joined popout).
#   2. Sluit de client netjes — `coverage::dump_if_enabled()` schrijft
#      `target/ui-coverage.json`.
#   3. Run dit script.
#
# Exit codes:
#   0 — actual matcht expected
#   1 — mismatch (diff getoond, toggelbaar via DIFF_TOOL)
#   2 — actual-dump ontbreekt (heeft client wel gedraaid met coverage feature?)

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

EXPECTED="$SCRIPT_DIR/ui-coverage-expected.json"
ACTUAL="$REPO_ROOT/target/ui-coverage.json"
DIFF_TOOL="${DIFF_TOOL:-diff}"

if [ ! -f "$ACTUAL" ]; then
    echo "ERROR: $ACTUAL niet gevonden." >&2
    echo "Build met --features ui-coverage (of debug) en draai alle render-paden vóór dit script." >&2
    exit 2
fi

if [ ! -f "$EXPECTED" ]; then
    echo "ERROR: $EXPECTED ontbreekt." >&2
    exit 2
fi

# jq -S sorteert keys zodat diff puur op inhoud gaat, niet op key-volgorde.
ACTUAL_NORM=$(jq -S . "$ACTUAL")
EXPECTED_NORM=$(jq -S . "$EXPECTED")

if [ "$ACTUAL_NORM" = "$EXPECTED_NORM" ]; then
    echo "ui-coverage: OK ($(jq 'length' "$EXPECTED") entries match)"
    exit 0
fi

echo "ui-coverage: MISMATCH" >&2
echo "Expected: $EXPECTED" >&2
echo "Actual:   $ACTUAL" >&2
echo "" >&2
"$DIFF_TOOL" <(printf '%s\n' "$EXPECTED_NORM") <(printf '%s\n' "$ACTUAL_NORM") >&2 || true
exit 1
