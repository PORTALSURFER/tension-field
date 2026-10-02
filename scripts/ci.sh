#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

usage() {
  cat <<'EOF'
Usage:
  scripts/ci.sh [--vst3] [--screenshots]

Runs the same checks locally that CI enforces:
  - scripts/policy-check.sh (if present)
  - cargo fmt --check
  - cargo clippy -D warnings
  - cargo test --locked
  - optional VST3 feature checks (when the macOS VST3 SDK is configured)
  - optional UI screenshot test (when supported)

Options:
  --vst3        Run the VST3 feature checks. Requires macOS and VST3_SDK_DIR
                pointing at an SDK containing pluginterfaces.
  --screenshots  Run `screenshot_renders_initial_ui` when the plugin supports the
                 screenshot harness (via a `screenshot-test` cargo feature).
EOF
}

want_vst3=0
want_screenshots=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --vst3) want_vst3=1; shift ;;
    --screenshots) want_screenshots=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ "${want_vst3}" == "1" ]]; then
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "[ci] --vst3 is supported on macOS only" >&2
    exit 2
  fi
  if [[ -z "${VST3_SDK_DIR:-}" || ! -d "${VST3_SDK_DIR}/pluginterfaces" ]]; then
    echo "[ci] --vst3 requires VST3_SDK_DIR to contain pluginterfaces" >&2
    exit 2
  fi
fi

if [[ -f scripts/policy-check.sh ]]; then
  bash scripts/policy-check.sh
fi

cargo fmt --all -- --check
if [[ "${want_vst3}" == "1" ]]; then
  cargo clippy --locked --all-targets --features vst3 -- -D warnings
  cargo test --locked --all --features vst3
else
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked --all
fi

if [[ "${want_screenshots}" == "1" ]]; then
  if ! grep -qE '^[[:space:]]*screenshot-test[[:space:]]*=' Cargo.toml; then
    echo "[ci] screenshot-test feature not defined; skipping screenshot harness"
    exit 0
  fi

  find target/ui-screenshots -type f -delete 2>/dev/null || true
  mkdir -p target/ui-screenshots

  screenshot_features=screenshot-test
  if [[ "${want_vst3}" == "1" ]]; then
    screenshot_features="screenshot-test,vst3"
  fi
  cargo test --locked --release --features "${screenshot_features}" gui::screenshot_tests -- --nocapture

  if ! compgen -G "target/ui-screenshots/*/initial-ui-*.png" >/dev/null; then
    echo "[ci] screenshot-test feature is enabled but no initial-ui screenshots were produced under target/ui-screenshots" >&2
    exit 1
  fi
fi
