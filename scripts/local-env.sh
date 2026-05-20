#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export TARGET_WORKSPACE="${TARGET_WORKSPACE:-${ROOT}/projects}"
export REPO_BASE_URI="${REPO_BASE_URI:-https://github.com/netsurf-browser}"
export TARGET_TOOLKIT="${TARGET_TOOLKIT:-gtk3}"

# docs/env.sh is intended to be sourced. It defines PREFIX, BUILD_PREFIX,
# PKG_CONFIG_PATH, PATH, and the ns-* helper functions used below.
# shellcheck source=/dev/null
source "${ROOT}/docs/env.sh"
