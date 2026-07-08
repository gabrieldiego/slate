#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_NAME="${1:-gtk}"
shift || true

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

"${ROOT}/scripts/build-slategenbind.sh"
make -C "${ROOT}" ${USE_CPUS} TARGET="${TARGET_NAME}" "$@"
