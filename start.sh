#!/bin/sh
# Start the existing development processes with project-pinned tools.
set -eu
cd -- "$(dirname -- "$0")"
if ! command -v mise >/dev/null 2>&1; then
  echo 'mise is required; see README.md for development setup.' >&2
  exit 1
fi
exec mise exec -- dekit mprocs --config mprocs.yaml --log-dir logs/dev "$@"
