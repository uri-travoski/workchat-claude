#!/usr/bin/env bash
# Debug build of the desktop binary with the UI embedded (no installer): target/debug/workchat
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/cc CC=/usr/bin/cc CXX=/usr/bin/c++
npx tauri build --debug --no-bundle
