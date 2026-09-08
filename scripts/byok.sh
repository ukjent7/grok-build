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

echo "==> cargo check (sampler + shell + sampling-types, incl. test targets)"
cargo check -p xai-grok-sampler -p xai-grok-shell -p xai-grok-sampling-types --all-targets

if [ "$SKIP_TESTS" -eq 0 ]; then
    echo "==> cargo test -p xai-grok-sampler"
    cargo test -p xai-grok-sampler
    echo "==> cargo test -p xai-grok-sampling-types (endpoint_trust only)"
    cargo test -p xai-grok-sampling-types -- endpoint_trust
    echo "==> cargo test -p xai-grok-shell (config/model layers)"
    cargo test -p xai-grok-shell -- agent::config agent::model_providers
fi

if [ "$RELEASE" -eq 1 ]; then
    echo "==> cargo build --release (pager binary)"
    cargo build -p xai-grok-pager-bin --release
    echo "binary: target/release/xai-grok-pager"
fi

echo "OK"
