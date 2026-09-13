#!/usr/bin/env bash
# Local equivalent of .github/workflows/byok-ci.yml for the BYOK changes.
# No local Rust? Just push and let CI run instead.
#
# Usage:
#   ./scripts/byok.sh                  # check + targeted tests
#   ./scripts/byok.sh --skip-tests     # compile only
#   ./scripts/byok.sh --release        # also build the release binary
set -euo pipefail

SKIP_TESTS=0
RELEASE=0
for arg in "$@"; do
    case "$arg" in
        --skip-tests) SKIP_TESTS=1 ;;
        --release) RELEASE=1 ;;
        -h|--help)
            echo "Usage: $0 [--skip-tests] [--release]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 1
            ;;
    esac
done

cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install rustup (https://rustup.rs), then:" >&2
    echo "  rustup toolchain install \"$(grep '^channel' rust-toolchain.toml | cut -d'\"' -f2)\"" >&2
    exit 1
fi

# xai-grok-tools-api runs proto codegen in its build script and needs protoc
# (via DotSlash bin/protoc, $PROTOC, or PATH). Warn early instead of failing late.
if ! command -v protoc >/dev/null 2>&1 && [ -z "${PROTOC:-}" ]; then
    echo "warning: protoc not on PATH and \$PROTOC unset; the build may fall back to DotSlash (needs 'dotslash' on PATH)." >&2
fi

echo "==> cargo check (sampler + shell + sampling-types + update + pager + pager-minimal + pager-render + pager-bin + locale + shared + shell-base + proto-build, incl. test targets)"
cargo check --locked -p xai-grok-sampler -p xai-grok-shell -p xai-grok-sampling-types -p xai-grok-update -p xai-grok-pager -p xai-grok-pager-minimal -p xai-grok-pager-render -p xai-grok-pager-bin -p xai-grok-locale -p xai-grok-shared -p xai-grok-shell-base -p xai-proto-build --all-targets --keep-going

if [ "$SKIP_TESTS" -eq 0 ]; then
    echo "==> i18n wrap check"
    python3 scripts/i18n/wrap-check.py check
    echo "==> cargo test -p xai-grok-locale"
    cargo test --locked -p xai-grok-locale --no-fail-fast
    echo "==> cargo test -p xai-grok-sampler"
    cargo test --locked -p xai-grok-sampler --no-fail-fast
    echo "==> cargo test -p xai-grok-update"
    cargo test --locked -p xai-grok-update --no-fail-fast
    echo "==> cargo test -p xai-grok-pager (usage status blocks)"
    cargo test --locked -p xai-grok-pager --no-fail-fast -- app::status_blocks
    echo "==> cargo test -p xai-grok-pager (localized mismatch guard)"
    cargo test --locked -p xai-grok-pager --no-fail-fast -- version_mismatch reconnect_guard
    echo "==> cargo test -p xai-grok-pager-minimal"
    cargo test --locked -p xai-grok-pager-minimal --no-fail-fast
    echo "==> cargo test -p xai-grok-pager-render (overlays only)"
    cargo test --locked -p xai-grok-pager-render --no-fail-fast -- image_overlay preview_overlay
    echo "==> cargo test -p xai-proto-build"
    cargo test --locked -p xai-proto-build --no-fail-fast
    echo "==> cargo test -p xai-grok-sampling-types (endpoint_trust only)"
    cargo test --locked -p xai-grok-sampling-types --no-fail-fast -- endpoint_trust
    echo "==> cargo test -p xai-grok-shell (config/model layers)"
    cargo test --locked -p xai-grok-shell --no-fail-fast -- agent::config agent::model_providers model_overrides
fi

# install.ps1 is validated in CI (pwsh parse step); mirror it locally when pwsh exists.
if command -v pwsh >/dev/null 2>&1; then
    echo "==> pwsh parse install.ps1"
    pwsh -NoProfile -Command '$errs=$null; [void][System.Management.Automation.Language.Parser]::ParseFile("install.ps1", [ref]$null, [ref]$errs); if ($errs.Count -ne 0) { $errs | ForEach-Object { Write-Error $_.Message }; exit 1 }'
else
    echo "==> skip install.ps1 parse (pwsh not on PATH)"
fi

if [ "$RELEASE" -eq 1 ]; then
    echo "==> cargo build --release (pager binary)"
    cargo build -p xai-grok-pager-bin --release
    echo "binary: target/release/xai-grok-pager"
fi

echo "OK"
