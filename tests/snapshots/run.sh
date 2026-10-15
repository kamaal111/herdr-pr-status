#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
mkdir -p target/snapshots tests/snapshots/baselines
docker build --platform linux/amd64 -t herdr-pr-status-snapshots tests/snapshots
baseline_mode=ro
if [ "${1:-}" = "--record" ]; then
    if [ -n "${CI:-}" ]; then
        echo 'Recording snapshots is disabled in CI' >&2
        exit 1
    fi
    baseline_mode=rw
fi
docker run --rm --platform linux/amd64 \
    -e CI="${CI:-}" \
    -v "$PWD:/workspace:ro" \
    -v "$PWD/target/snapshots:/output" \
    -v "$PWD/tests/snapshots/baselines:/baselines:$baseline_mode" \
    herdr-pr-status-snapshots sh -c 'cargo build --locked --release && python3 tests/snapshots.py "$@"' sh "$@"
