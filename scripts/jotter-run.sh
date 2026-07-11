#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

JOTTER="${ROOT}/jotter"
TIMEOUT_SECS=30
WRAPPER_ARGS=()
PLAN=""

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] PLAN.yaml

Run a Jotter browser-driver plan with a stable command name for approvals.

Options:
  -m, --jotter PATH     Jotter binary to run
                        default: ${JOTTER}
  --timeout SECONDS     Kill the driver after this many seconds
                        default: ${TIMEOUT_SECS}; use 0 to disable
  -w, --wrapper ARG     Wrapper argument passed to jotter_driver.py
                        may be repeated
  -h, --help            Show this help
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

while [ "$#" -gt 0 ]; do
	case "$1" in
		-m|--jotter)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			JOTTER="$2"
			shift 2
			;;
		--timeout)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			TIMEOUT_SECS="$2"
			shift 2
			;;
		--timeout=*)
			TIMEOUT_SECS="${1#*=}"
			shift
			;;
		-w|--wrapper)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			WRAPPER_ARGS+=("$2")
			shift 2
			;;
		-h|--help)
			usage
			exit 0
			;;
		-*)
			die "Unknown option: $1"
			;;
		*)
			[ -z "${PLAN}" ] || die "Only one test plan may be supplied"
			PLAN="$1"
			shift
			;;
	esac
done

[ -n "${PLAN}" ] || die "Missing test plan"
[ -x "${JOTTER}" ] || die "Jotter binary is not executable: ${JOTTER}"
[ -r "${PLAN}" ] || die "Test plan is not readable: ${PLAN}"

case "${TIMEOUT_SECS}" in
	''|*[!0-9]*)
		die "Timeout must be a non-negative integer"
		;;
esac

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh" >/dev/null || exit 1

export PYTHONUNBUFFERED="${PYTHONUNBUFFERED:-1}"

CMD=("${ROOT}/test/jotter_driver.py" -m "${JOTTER}" -t "${PLAN}")
if [ "${#WRAPPER_ARGS[@]}" -gt 0 ]; then
	CMD+=(-w "${WRAPPER_ARGS[*]}")
fi

if [ "${TIMEOUT_SECS}" -gt 0 ]; then
	exec timeout "${TIMEOUT_SECS}" "${CMD[@]}"
fi

exec "${CMD[@]}"
