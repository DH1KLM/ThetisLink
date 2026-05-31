#!/usr/bin/env bash
#
# sync-to-public.sh — stage allowlisted files for push to public repo.
#
# Usage:
#   scripts/sync-to-public.sh <staging-dir>
#
# The staging dir must NOT exist. It is created fresh each run. After
# the copy, the same forbidden-pattern scan that the public CI-gate
# runs is applied to the staging dir; any match aborts the sync and
# removes the staging dir.
#
# This script is the ONLY sanctioned path from the private repo to
# the public repo. Anything not on the explicit allowlist below does
# not ship.

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <staging-dir>" >&2
    exit 2
fi

STAGING="$1"
if [ -e "$STAGING" ]; then
    echo "Error: $STAGING already exists. Remove first." >&2
    exit 2
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
mkdir -p "$STAGING"

# -----------------------------------------------------------------
# Allowlist
# -----------------------------------------------------------------

ROOT_FILES=(
    README.md
    LICENSE
    LICENSE-COMMERCIAL.md
    NOTICE.md
    ATTRIBUTION.md
    Credits.md
    Installation.md
    Installatie.md
    Technical-Reference.md
    Technische-Referentie.md
    User-Manual.md
    User-Manual-EN.md
    Cargo.toml
    Cargo.lock
    about.toml
    about.hbs
)

SOURCE_DIRS=(
    sdr-remote-core
    sdr-remote-logic
    sdr-remote-server
    sdr-remote-client
)

ANDROID_DIR="sdr-remote-android"

SOURCE_EXCLUDES=(
    'target'
    '*.so'
    '*.apk'
    '*.exe'
    '*.pdb'
)

ANDROID_EXCLUDES=(
    'target'
    'android/target'
    'android/sdr-remote-android'
    'android/app/build'
    'android/.gradle'
    'android/local.properties'
    'android/INSTALLATIE.md'
    '*.so'
    '*.apk'
    '*.exe'
    '*.pdb'
)

COMPLIANCE_FILES=(
    compliance/sbom.spdx.json
    compliance/THIRD-PARTY-LICENSES.html
    compliance/licenses/UFL-1.0.txt
)

GITHUB_WORKFLOW="\.github/workflows/guard.yml"

# -----------------------------------------------------------------
# Copy
# -----------------------------------------------------------------

fail_with() {
    echo "sync failed: $1" >&2
    rm -rf "$STAGING"
    exit 1
}

# Root files
for f in "${ROOT_FILES[@]}"; do
    if [ ! -f "$REPO_ROOT/$f" ]; then
        fail_with "missing root file: $f"
    fi
    cp "$REPO_ROOT/$f" "$STAGING/$f"
done

# .gitignore.public -> .gitignore (clean public version)
if [ ! -f "$REPO_ROOT/.gitignore.public" ]; then
    fail_with "missing .gitignore.public"
fi
cp "$REPO_ROOT/.gitignore.public" "$STAGING/.gitignore"

# Source dirs (tar pipe with excludes)
for d in "${SOURCE_DIRS[@]}"; do
    if [ ! -d "$REPO_ROOT/$d" ]; then
        fail_with "missing source dir: $d"
    fi
    mkdir -p "$STAGING/$d"
    ex_args=()
    for e in "${SOURCE_EXCLUDES[@]}"; do ex_args+=(--exclude="$e"); done
    (cd "$REPO_ROOT/$d" && tar -c "${ex_args[@]}" .) | (cd "$STAGING/$d" && tar -x)
done

# Android dir with extra excludes
mkdir -p "$STAGING/$ANDROID_DIR"
ex_args=()
for e in "${ANDROID_EXCLUDES[@]}"; do ex_args+=(--exclude="$e"); done
(cd "$REPO_ROOT/$ANDROID_DIR" && tar -c "${ex_args[@]}" .) | (cd "$STAGING/$ANDROID_DIR" && tar -x)

# Compliance files
for f in "${COMPLIANCE_FILES[@]}"; do
    if [ ! -f "$REPO_ROOT/$f" ]; then
        fail_with "missing compliance file: $f"
    fi
    mkdir -p "$(dirname "$STAGING/$f")"
    cp "$REPO_ROOT/$f" "$STAGING/$f"
done

# CI-gate workflow
if [ ! -f "$REPO_ROOT/.github/workflows/guard.yml" ]; then
    fail_with "missing .github/workflows/guard.yml"
fi
mkdir -p "$STAGING/.github/workflows"
cp "$REPO_ROOT/.github/workflows/guard.yml" "$STAGING/.github/workflows/guard.yml"

# -----------------------------------------------------------------
# Forbidden-pattern scan (same as CI-gate)
# -----------------------------------------------------------------

FORBIDDEN_PATTERNS=(
    '\bclaude\b'
    '\banthropic\b'
    'Co-Authored-By: Claude'
    '\bAI-[123]\b'
    'ChatGPT'
    'OpenAI'
    'Copilot'
    'Gemini'
    '\bGPT\b'
    '\bLLM\b'
    'AI-assisted'
    'AI-powered'
    'artificial intelligence'
)

FORBIDDEN_PATHS=(
    CLAUDE.md
    Faseplan.md
    RELEASE.md
    docs/patch-briefs
    docs/audit
    .claude
)

scan_fail=0

for path in "${FORBIDDEN_PATHS[@]}"; do
    if [ -e "$STAGING/$path" ]; then
        echo "FORBIDDEN PATH present in staging: $path" >&2
        scan_fail=1
    fi
done

for pat in "${FORBIDDEN_PATTERNS[@]}"; do
    matches=$(grep -rniE "$pat" "$STAGING" \
        --include='*.rs' --include='*.kt' --include='*.md' \
        --include='*.toml' --include='*.sh' --include='*.yml' --include='*.yaml' \
        --include='*.json' --include='*.txt' --include='*.hbs' \
        --include='.gitignore' \
        --exclude='guard.yml' \
        2>/dev/null || true)
    if [ -n "$matches" ]; then
        echo "FORBIDDEN PATTERN '$pat':" >&2
        echo "$matches" >&2
        scan_fail=1
    fi
done

if [ $scan_fail -ne 0 ]; then
    echo "" >&2
    fail_with "forbidden content in staging — clean private branch first"
fi

echo "sync ok: staging ready at $STAGING"
