#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

if ! pkg-config --exists check; then
	cat >&2 <<'EOF'
Missing unit-test dependency: check

Install the Check C unit test framework development package, then rerun:

  Debian/Ubuntu: sudo apt-get install check
  Fedora:        sudo dnf install check-devel
  openSUSE:      sudo zypper install check-devel
EOF
	exit 1
fi

if [ ! -d "${TARGET_WORKSPACE}" ] || [ ! -x "${BUILD_PREFIX}/bin/slategenbind" ]; then
	"${ROOT}/scripts/fetch-deps.sh"
	"${ROOT}/scripts/build-libs.sh"
fi

make -C "${ROOT}" ${USE_CPUS} test "$@"
