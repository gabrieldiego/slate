#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

if [ ! -d "${BUILD_PREFIX}/share/slate-buildsystem" ]; then
	slate-make-tools install
fi

make -C "${ROOT}/tools/slategenbind" \
	PREFIX="${BUILD_PREFIX}" \
	NSSHARED="${BUILD_PREFIX}/share/slate-buildsystem" \
	HOST="${BUILD}" \
	${USE_CPUS} \
	install
