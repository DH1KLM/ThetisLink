#!/usr/bin/env bash
# scripts/evidence-scan.sh
#
# Publication-gate: blocks the push to the new clean public repo when any of
# the NereusSDR-notice §4 bewijs-patronen (see
# docs/patch-briefs/INTAKE-public-release-compliance-remediation.md §10.7)
# appear in the tree.
#
# Each of the 10 patterns (P1..P10) is implemented 1:1 with an explicit label
# and a machine-readable output format: `<patternId>\t<severity>\t<file>:<line>\t<context>`
# on stdout; human-friendly summary on stderr.
#
# Uses GNU grep -E (available in Git Bash / Linux / macOS), no ripgrep dep.
#
# Usage:
#     scripts/evidence-scan.sh            run scan on current working tree
#     scripts/evidence-scan.sh --self-test   run synthetic positive/negative
#                                            fixtures to verify detector-logic
#
# Exit codes:
#     0  no BLOCK-severity hits
#     1  at least one BLOCK-severity hit — publication-gate FAIL
#     2  --self-test failed
#     3  invocation error

set -u
export LC_ALL=C

HITS_BLOCK=0
HITS_REVIEW=0

# Root for source scans. Scripts run from any cwd; resolve repo-root.
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

# Paths that hold our own Rust source code — scanned for patterns.
# Anything outside these is out of scope for the scan (target/, release/,
# compliance-archive, docs/, docs-book/).
src_files() {
    find "$REPO_ROOT" \
        \( -path "$REPO_ROOT/target" -o \
           -path "$REPO_ROOT/release" -o \
           -path "$REPO_ROOT/compliance-archive*" -o \
           -path "$REPO_ROOT/docs" -o \
           -path "$REPO_ROOT/docs-book" -o \
           -path "$REPO_ROOT/scripts" -o \
           -path "*/.git" \) -prune -o \
        \( -name '*.rs' -o -name 'build.rs' \) -type f -print
}

emit_hit() {
    local pid="$1" sev="$2" loc="$3" ctx="$4"
    printf '%s\t%s\t%s\t%s\n' "$pid" "$sev" "$loc" "$ctx"
    case "$sev" in
        BLOCK)  HITS_BLOCK=$((HITS_BLOCK + 1));;
        REVIEW) HITS_REVIEW=$((HITS_REVIEW + 1));;
    esac
}

# Run `grep -nE` on all source files with given ERE pattern and emit hits.
# Args: PATTERN_ID SEVERITY grep_options... ERE_PATTERN
grep_scan() {
    local pid="$1" sev="$2"; shift 2
    local files
    files=$(src_files)
    [ -z "$files" ] && return 0
    # shellcheck disable=SC2086
    echo "$files" | xargs grep -nE "$@" 2>/dev/null | while IFS= read -r line; do
        # Format: file:line:match-content
        local loc ctx
        loc=$(echo "$line" | awk -F: '{print $1":"$2}')
        ctx=$(echo "$line" | cut -d: -f3- | sed 's/^ *//')
        printf '%s\t%s\t%s\t%s\n' "$pid" "$sev" "$loc" "$ctx"
    done
}

# Wrapper that also increments counters from output. Bash subshell in pipe
# breaks counter increments; we run the scan to a temp file instead.
run_pattern_scan() {
    local pid="$1" sev="$2"; shift 2
    local tmp
    tmp=$(mktemp)
    local files
    files=$(src_files)
    [ -z "$files" ] && { rm -f "$tmp"; return 0; }
    # shellcheck disable=SC2086
    echo "$files" | xargs grep -nE "$@" 2>/dev/null > "$tmp" || true
    local count
    count=$(wc -l < "$tmp" | tr -d ' ')
    if [ "$count" -gt 0 ]; then
        while IFS= read -r line; do
            local loc ctx
            loc=$(echo "$line" | awk -F: '{print $1":"$2}')
            ctx=$(echo "$line" | cut -d: -f3- | sed 's/^ *//')
            emit_hit "$pid" "$sev" "$loc" "$ctx"
        done < "$tmp"
    fi
    rm -f "$tmp"
}

# P1 — File+line reference to Thetis C# source
scan_P1() {
    run_pattern_scan "P1" "BLOCK" '[A-Za-z_][A-Za-z0-9_]*\.cs:[0-9]+'
}

# P2 — Thetis C# class names
scan_P2() {
    run_pattern_scan "P2" "BLOCK" \
        '(TCIServer|MemoryForm|MemoryRecord|MemoryList|DXMemList|DXMemRecord|DSPMode|ConsoleForm|MeterManager|NetworkIO|SetupForm|AmpView)'
}

# P3 — Derivative phrases (case-insensitive via -i)
scan_P3() {
    run_pattern_scan "P3" "BLOCK" -i \
        '(matches Thetis|port(ed)? from Thetis|copy of Thetis|based on Thetis|cloned from Thetis|replicates Thetis|mirrors Thetis|same as Thetis)'
}

# P4 — WDSP enum/constant names
scan_P4() {
    run_pattern_scan "P4" "BLOCK" \
        '(AVG_SIGNAL_STRENGTH|SIGNAL_STRENGTH|Display_Buffer|RXA_[A-Z_]+|TXA_[A-Z_]+)'
}

# P5 — WDSP / PowerSDR / "Flex native" mentions
scan_P5() {
    run_pattern_scan "P5" "REVIEW" -i '(WDSP|PowerSDR|Flex[- ]?native)'
}

# P6 — Thetis working-tree path references
scan_P6() {
    # Use alternation for both path separators
    run_pattern_scan "P6" "BLOCK" 'Project Files[/\].*Source[/\].*Console|Thetis[/\]Project Files'
}

# P7 — Specific Thetis-unique parameter values. The "Peak, log recursive Nms"
# phrasing with a specific ms value is a Thetis-specific configuration name.
# Blackman-Harris *alone* is public DSP terminology (Harris 1978) and is NOT
# derivative evidence by itself — P3 still catches "matches Thetis …
# Blackman-Harris" in a comparison context.
scan_P7() {
    run_pattern_scan "P7" "BLOCK" -i \
        'Peak,[[:space:]]*log[[:space:]]+recursive[[:space:]]*[0-9]+[[:space:]]*ms'
}

# P8 — Commit-message evidence in git log
scan_P8() {
    if ! git -C "$REPO_ROOT" rev-parse --git-dir >/dev/null 2>&1; then return 0; fi
    local matches
    matches=$(git -C "$REPO_ROOT" log --format='%H|%s' 2>/dev/null | \
        grep -iE '(port from Thetis|copied from Thetis|ported from Thetis)' || true)
    if [ -n "$matches" ]; then
        while IFS='|' read -r sha msg; do
            emit_hit "P8" "BLOCK" "commit:${sha:0:12}" "$msg"
        done <<< "$matches"
    fi
}

# P9 — Rust struct names suggesting C# pattern (REVIEW, not BLOCK)
scan_P9() {
    run_pattern_scan "P9" "REVIEW" \
        'struct[[:space:]]+[A-Za-z_][A-Za-z0-9_]*(Form|Manager|Controller)[[:space:]<{]'
}

# P10 — Files without SPDX header containing upstream project references
scan_P10() {
    local f head_txt rest_txt
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        head_txt=$(head -5 "$f")
        if ! echo "$head_txt" | grep -q 'SPDX-License-Identifier:'; then
            rest_txt=$(cat "$f")
            if echo "$rest_txt" | grep -qiE '(Thetis|WDSP|PowerSDR|OpenHPSDR|FlexRadio|MW0LGE|NR0V|G8NJJ|MI0BOT)'; then
                local rel="${f#$REPO_ROOT/}"
                emit_hit "P10" "BLOCK" "$rel:1" "(no SPDX header; contains upstream-project reference)"
            fi
        fi
    done < <(src_files)
}

run_scan() {
    echo "=== ThetisLink evidence-scan — $(date -u '+%Y-%m-%dT%H:%M:%SZ') ===" >&2
    echo "# Output format: PATTERN_ID<TAB>SEVERITY<TAB>file:line<TAB>context" >&2
    scan_P1; scan_P2; scan_P3; scan_P4; scan_P5
    scan_P6; scan_P7; scan_P8; scan_P9; scan_P10
    echo "=== Summary: $HITS_BLOCK BLOCK / $HITS_REVIEW REVIEW ===" >&2
    if [ "$HITS_BLOCK" -gt 0 ]; then
        echo "=== FAIL: $HITS_BLOCK BLOCK-severity hit(s). Publication gate DENIED. ===" >&2
        return 1
    fi
    if [ "$HITS_REVIEW" -gt 0 ]; then
        echo "=== PASS with $HITS_REVIEW REVIEW hit(s). Inspect manually. ===" >&2
    else
        echo "=== PASS: no patterns detected. Publication gate OK. ===" >&2
    fi
    return 0
}

# --- Self-test: synthetic fixtures that exercise each pattern --------------

self_test() {
    local tmpdir; tmpdir=$(mktemp -d) || { echo "cannot mktemp" >&2; exit 3; }
    local original_script="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
    trap "rm -rf '$tmpdir'" EXIT
    mkdir -p "$tmpdir/sdr-remote-test/src"
    cd "$tmpdir" || exit 3

    local failures=0
    assert_fires() {
        local pid="$1" file="$2" expect="$3"
        local out
        out=$(bash "$original_script" 2>/dev/null | grep "^${pid}	" || true)
        if [ "$expect" = "YES" ] && [ -z "$out" ]; then
            echo "FAIL: $pid should fire on $file but did not" >&2
            failures=$((failures + 1))
        elif [ "$expect" = "NO" ] && [ -n "$out" ]; then
            echo "FAIL: $pid should NOT fire on $file but did: $out" >&2
            failures=$((failures + 1))
        else
            echo "OK: $pid $expect on $file" >&2
        fi
    }

    # Trick: self-test runs from $tmpdir but script resolves REPO_ROOT as
    # parent of script dir. The script is in sdr-remote/scripts/, which has
    # its own REPO_ROOT (sdr-remote/). The self-test creates fixtures in
    # $tmpdir/sdr-remote-test/src/ — but the script won't scan there.
    # So we copy the script into a fake repo layout and set REPO_ROOT via
    # symlink. Simpler: override src_files via env — but script doesn't
    # support that. Cleanest: use isolated copy.
    local fake_root="$tmpdir/fake-repo"
    mkdir -p "$fake_root/scripts" "$fake_root/sdr-remote-test/src"
    cp "$original_script" "$fake_root/scripts/evidence-scan.sh"
    chmod +x "$fake_root/scripts/evidence-scan.sh"

    # Helper: write a file, run scan, assert
    run_in_fake() {
        cd "$fake_root" || return
        bash "$fake_root/scripts/evidence-scan.sh" 2>/dev/null
    }

    fire_check() {
        local pid="$1" label="$2" expect="$3"
        local out
        out=$(run_in_fake | grep "^${pid}	" || true)
        if [ "$expect" = "YES" ] && [ -z "$out" ]; then
            echo "FAIL: $pid ($label) should fire but did not" >&2
            failures=$((failures + 1))
            return 1
        elif [ "$expect" = "NO" ] && [ -n "$out" ]; then
            echo "FAIL: $pid ($label) should NOT fire but did: $out" >&2
            failures=$((failures + 1))
            return 1
        fi
        echo "OK: $pid $expect on $label" >&2
        return 0
    }

    # Fixture-per-case, cleanup between each
    mk_and_test() {
        local pid="$1" label="$2" expect="$3" content="$4"
        local f="$fake_root/sdr-remote-test/src/fixture.rs"
        printf '%s\n' "$content" > "$f"
        fire_check "$pid" "$label" "$expect"
        rm -f "$f"
    }

    mk_and_test "P1" "P1-pos" "YES" '// See TCIServer.cs:4310 for upstream
fn main() {}'
    mk_and_test "P1" "P1-neg" "NO" '// See Kenwood CAT spec
fn main() {}'

    mk_and_test "P2" "P2-pos" "YES" '// Like MemoryRecord upstream
fn main() {}'
    mk_and_test "P2" "P2-neg" "NO" '// My own record struct
fn main() {}'

    mk_and_test "P3" "P3-pos" "YES" '// ported from Thetis for this purpose
fn main() {}'
    mk_and_test "P3" "P3-neg" "NO" '// interoperates with Thetis via TCI
fn main() {}'

    mk_and_test "P4" "P4-pos" "YES" '// avgdBm = AVG_SIGNAL_STRENGTH
fn main() {}'
    mk_and_test "P4" "P4-neg" "NO" '// time-domain RMS averaged power
fn main() {}'

    mk_and_test "P5" "P5-pos" "YES" '// WDSP uses this approach
fn main() {}'
    mk_and_test "P5" "P5-neg" "NO" '// DSP uses this approach
fn main() {}'

    mk_and_test "P7" "P7-pos-specific" "YES" '// Peak, log recursive 120ms similar
fn main() {}'
    mk_and_test "P7" "P7-neg-blackman-harris-only" "NO" '// see Blackman-Harris window for this
fn main() {}'
    mk_and_test "P7" "P7-neg-custom" "NO" '// custom Hann window
fn main() {}'

    # P9 — REVIEW only (struct naming)
    mk_and_test "P9" "P9-pos" "YES" 'struct MemoryManager {}'
    mk_and_test "P9" "P9-neg" "NO" 'struct Foo {}'

    # P10 — missing SPDX AND upstream ref
    mk_and_test "P10" "P10-pos" "YES" 'fn talk_to_Thetis() {}'
    mk_and_test "P10" "P10-neg-has-spdx" "NO" '// SPDX-License-Identifier: GPL-2.0-or-later
fn talk_to_Thetis() {}'
    mk_and_test "P10" "P10-neg-no-upstream" "NO" 'fn own_code() {}'

    if [ "$failures" -gt 0 ]; then
        echo "=== SELF-TEST FAILED ($failures failures) ===" >&2
        exit 2
    fi
    echo "=== SELF-TEST PASSED (all fixtures correctly classified) ===" >&2
    exit 0
}

case "${1:-}" in
    --self-test) self_test ;;
    "")          run_scan; exit $? ;;
    *)           echo "Usage: $0 [--self-test]" >&2; exit 3 ;;
esac
