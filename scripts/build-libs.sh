#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh"

# Several NetSurf support libraries generate lexer/parser sources into build
# directories whose creation is not fully ordered for a clean parallel build.
# Build the support repos serially; the browser build itself remains parallel.
USE_CPUS="-j1"
ns-make-tools install
"${ROOT}/scripts/build-slategenbind.sh"
ns-make-libs install
