#!/usr/bin/env bash
set -eo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BINARY="${ROOT}/slategtk3"
BATCH=0

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] [--] [BROWSER_ARG...]

Run a Slate browser binary under gdb.

Options:
  -b, --binary PATH     Binary to run
                        default: ${BINARY}
  --batch              Run, then print a backtrace/register dump after exit/crash
  -h, --help           Show this help
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

while [ "$#" -gt 0 ]; do
	case "$1" in
		-b|--binary)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			BINARY="$2"
			shift 2
			;;
		--binary=*)
			BINARY="${1#*=}"
			shift
			;;
		--batch)
			BATCH=1
			shift
			;;
		-h|--help)
			usage
			exit 0
			;;
		--)
			shift
			break
			;;
		-*)
			die "Unknown option: $1"
			;;
		*)
			break
			;;
	esac
done

[ -x "${BINARY}" ] || die "Binary is not executable: ${BINARY}"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh" >/dev/null || exit 1

COMMON_EX=(
	-ex "set pagination off"
	-ex "set debuginfod enabled off"
)

if [ "${BATCH}" -eq 1 ]; then
	exec gdb -q \
		"${COMMON_EX[@]}" \
		-ex "run" \
		-ex "thread apply all bt 50" \
		-ex "info registers" \
		--args "${BINARY}" "$@"
fi

exec gdb -q "${COMMON_EX[@]}" --args "${BINARY}" "$@"
