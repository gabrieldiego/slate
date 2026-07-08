#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export TARGET_WORKSPACE="${TARGET_WORKSPACE:-${ROOT}/projects}"
export REPO_BASE_URI="${REPO_BASE_URI:-https://github.com/gabrieldiego}"
export QUICKJS_REPO_URI="${QUICKJS_REPO_URI:-https://github.com/bellard/quickjs}"
export QUICKJS_REF="${QUICKJS_REF:-04be246001599f5995fa2f2d8c91a0f198d3f34c}"
export TARGET_TOOLKIT="${TARGET_TOOLKIT:-gtk3}"

# docs/env.sh is intended to be sourced. It defines PREFIX, BUILD_PREFIX,
# PKG_CONFIG_PATH, PATH, and the slate-* helper functions used below. Disable
# errexit while sourcing it because cross-compiler probing intentionally tries
# command names that may not exist before it finds the right one.
slate_local_env_errexit=0
case $- in
	*e*)
		slate_local_env_errexit=1
		set +e
		;;
esac

# shellcheck source=/dev/null
source "${ROOT}/docs/env.sh"
slate_local_env_status=$?

if [ "${slate_local_env_errexit}" -eq 1 ]; then
	set -e
fi
unset slate_local_env_errexit

if [ "${slate_local_env_status}" -ne 0 ]; then
	return "${slate_local_env_status}" 2>/dev/null || exit "${slate_local_env_status}"
fi
unset slate_local_env_status
