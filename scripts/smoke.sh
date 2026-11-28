#!/usr/bin/env bash
# Run one desktop smoke scenario: scripts/smoke.sh <test|world|nemi|cave|crossing> [game arguments...]
#
# The scenarios render the real game and need the GPU. Agent sandboxes
# (Codex's bubblewrap sandbox, for one) present a synthetic /dev without
# /dev/dri, so a sandboxed run dies with Bevy's "could not detect a GPU"
# even on a host with a working GPU. Check for the devices first and say
# that plainly, instead of letting the misleading error through: this
# script must run outside the sandbox (the issue-to-PR execpolicy rules
# allow it in exactly this form).
set -euo pipefail

scenario="${1:-}"
case "$scenario" in
    test | world | nemi | cave | crossing) ;;
    *)
        echo "usage: scripts/smoke.sh <test|world|nemi|cave|crossing> [game arguments...]" >&2
        exit 2
        ;;
esac

if [ ! -e /dev/dri ]; then
    echo "smoke.sh: no GPU devices at /dev/dri - this process is running in a" >&2
    echo "smoke.sh: sandbox that hides the GPU. Rerun outside the sandbox" >&2
    echo "smoke.sh: (escalated); do not report the scenario as un-runnable" >&2
    echo "smoke.sh: on this host based on this failure." >&2
    exit 1
fi

shift
cd "$(dirname -- "$0")/.."
exec cargo run --locked --features smoke -- --smoke-"$scenario" "$@"
