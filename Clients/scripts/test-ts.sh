#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
rm -rf .test-build && mkdir -p .test-build
for t in tests/*.test.ts; do npx esbuild "$t" --bundle --platform=node --format=cjs --outfile=".test-build/$(basename $t .ts).cjs" --log-level=error || exit 1; done
node --test .test-build/
