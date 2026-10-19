#!/bin/sh
# Fail fast, with one sentence, when TideTable's CLI is not on PATH.
#
# Used by an agent or a human before driving the plan; the rest of the
# commands assume `tidetable`/`tt` are installed.
set -eu

if ! command -v tidetable >/dev/null 2>&1; then
    echo "TideTable is not installed; install it from ~/git_repos/clawffice/TideTable with 'uv tool install --editable ~/git_repos/clawffice/TideTable' (or pipx)." >&2
    exit 1
fi

exec tidetable "$@"
