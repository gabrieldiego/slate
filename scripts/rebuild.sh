#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_NAME="${1:-gtk}"
shift || true

"${ROOT}/scripts/fetch-deps.sh"
"${ROOT}/scripts/build-libs.sh"
"${ROOT}/scripts/build.sh" "${TARGET_NAME}" "$@"
