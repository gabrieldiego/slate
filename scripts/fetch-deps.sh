#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

slate-clone -d -s "$@"

if [ -d "${TARGET_WORKSPACE}/quickjs/.git" ]; then
	git -C "${TARGET_WORKSPACE}/quickjs" remote set-url origin "${QUICKJS_REPO_URI}"
	git -C "${TARGET_WORKSPACE}/quickjs" fetch origin
else
	git clone "${QUICKJS_REPO_URI}" "${TARGET_WORKSPACE}/quickjs"
fi

git -C "${TARGET_WORKSPACE}/quickjs" checkout --detach "${QUICKJS_REF}"

quickjs_patch="${ROOT}/content/handlers/javascript/quickjs/quickjs-slate-compat.patch"
if [ -f "${quickjs_patch}" ]; then
	if git -C "${TARGET_WORKSPACE}/quickjs" apply --check "${quickjs_patch}"; then
		git -C "${TARGET_WORKSPACE}/quickjs" apply "${quickjs_patch}"
	elif git -C "${TARGET_WORKSPACE}/quickjs" apply --reverse --check "${quickjs_patch}"; then
		echo "QuickJS Slate compatibility patch already applied"
	else
		echo "Unable to apply QuickJS Slate compatibility patch" >&2
		exit 1
	fi
fi
