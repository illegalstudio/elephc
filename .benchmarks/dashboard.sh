#!/usr/bin/env bash
# Launch the local benchmark dashboard.
# Thin wrapper around dashboard.py so you don't have to remember the path.
# Any arguments are forwarded (e.g. --port 9000, --no-open).
#
#   ./.benchmarks/dashboard.sh
#   ./.benchmarks/dashboard.sh --port 9000

set -euo pipefail

# Resolve the directory this script lives in, so it works from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PYTHON="$(command -v python3 || command -v python || true)"
if [ -z "$PYTHON" ]; then
  echo "error: python3 not found on PATH" >&2
  exit 1
fi

exec "$PYTHON" "$SCRIPT_DIR/dashboard.py" "$@"
