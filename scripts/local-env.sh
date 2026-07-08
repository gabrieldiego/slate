#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export TARGET_WORKSPACE="${TARGET_WORKSPACE:-${ROOT}/projects}"
export REPO_BASE_URI="${REPO_BASE_URI:-https://github.com/gabrieldiego}"
export QUICKJS_REPO_URI="${QUICKJS_REPO_URI:-https://github.com/bellard/quickjs}"
export QUICKJS_REF="${QUICKJS_REF:-04be246001599f5995fa2f2d8c91a0f198d3f34c}"
export TARGET_TOOLKIT="${TARGET_TOOLKIT:-gtk3}"

# docs/env.sh is intended to be sourced. It defines PREFIX, BUILD_PREFIX,
# PKG_CONFIG_PATH, PATH, and the slate-* helper functions used below.
# shellcheck source=/dev/null
source "${ROOT}/docs/env.sh"
