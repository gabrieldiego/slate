#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_NAME="${1:-gtk}"
shift || true

rm -rf "${ROOT}/build" \
       "${ROOT}/projects" \
       "${ROOT}/slategtk2" \
       "${ROOT}/slategtk3" \
       "${ROOT}/slatefb" \
       "${ROOT}/jotter" \
       "${ROOT}/slateqt"

"${ROOT}/scripts/rebuild.sh" "${TARGET_NAME}" "$@"
