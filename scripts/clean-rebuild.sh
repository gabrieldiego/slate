#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_NAME="${1:-gtk}"
shift || true

rm -rf "${ROOT}/build" \
       "${ROOT}/projects" \
       "${ROOT}/nsgtk2" \
       "${ROOT}/nsgtk3" \
       "${ROOT}/nsfb" \
       "${ROOT}/nsmonkey" \
       "${ROOT}/nsqt"

"${ROOT}/scripts/rebuild.sh" "${TARGET_NAME}" "$@"
