#!/usr/bin/env bash
# CI / disposable test session only. Uses a virtual sink, never a physical output.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --no-run
pulseaudio --start --exit-idle-time=-1
module_id="$(pactl load-module module-null-sink sink_name=translator_ci sink_properties=device.description=Translator-CI)"
playback_pid=""
cleanup() {
  if [[ -n "$playback_pid" ]]; then kill "$playback_pid" 2>/dev/null || true; fi
  pactl unload-module "$module_id" || true
}
trap cleanup EXIT
python3 scripts/generate-test-tone.py
paplay --device=translator_ci artifacts/test-tone.wav &
playback_pid="$!"
INTERVIEW_TRANSLATOR_TEST_AUDIO_DEVICE=pulseaudio:translator_ci cargo test --locked --manifest-path src-tauri/Cargo.toml --lib platform_loopback_smoke -- --ignored
