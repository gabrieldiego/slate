#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

if [ ! -d "${BUILD_PREFIX}/share/slate-buildsystem" ]; then
	slate-make-tools install
fi

BUILD_CC="${BUILD_CC:-}"
if [ -z "${BUILD_CC}" ]; then
	for cc_candidate in "${BUILD}-gcc" "${BUILD}-cc" cc gcc; do
		if command -v "${cc_candidate}" >/dev/null 2>&1; then
			if [ "$("${cc_candidate}" -dumpmachine 2>/dev/null)" = "${BUILD}" ]; then
				BUILD_CC="$(command -v "${cc_candidate}")"
				break
			fi
		fi
	done
fi

if [ -z "${BUILD_CC}" ]; then
	echo "Unable to locate build compiler for BUILD=${BUILD}" >&2
	exit 1
fi

make -C "${ROOT}/tools/slategenbind" \
	PREFIX="${BUILD_PREFIX}" \
	NSSHARED="${BUILD_PREFIX}/share/slate-buildsystem" \
	HOST="${BUILD}" \
	CC="${BUILD_CC}" \
	${USE_CPUS} \
	install
