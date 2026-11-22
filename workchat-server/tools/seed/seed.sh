#!/usr/bin/env bash
# Runs the seed script with its own virtual environment (created on first use, git-ignored): no system packages are touched.
#   ./seed.sh --server http://192.168.8.101:18600 --owner-password 'pw' --password 'user-pw'
set -euo pipefail
cd "$(dirname "$0")"
if [ ! -x .venv/bin/python ]; then
  python3 -m venv .venv
  .venv/bin/pip install -q -r requirements.txt
fi
exec .venv/bin/python seed.py "$@"
