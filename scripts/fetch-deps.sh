#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

ns-clone -d -s "$@"

if [ -d "${TARGET_WORKSPACE}/duktape/.git" ]; then
	git -C "${TARGET_WORKSPACE}/duktape" remote set-url origin "${DUKTAPE_REPO_URI}"
else
	git clone "${DUKTAPE_REPO_URI}" "${TARGET_WORKSPACE}/duktape"
fi
