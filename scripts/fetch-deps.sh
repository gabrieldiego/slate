#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

ns-clone -d -s "$@"

if [ -d "${TARGET_WORKSPACE}/quickjs/.git" ]; then
	git -C "${TARGET_WORKSPACE}/quickjs" remote set-url origin "${QUICKJS_REPO_URI}"
else
	git clone "${QUICKJS_REPO_URI}" "${TARGET_WORKSPACE}/quickjs"
fi
