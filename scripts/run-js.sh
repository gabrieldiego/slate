#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

WAIT_SECONDS=30
LOG_FILE="${ROOT}/jotter-js-dry-run.log"
ERROR_FILTER='^(warn|error|die)( |$)|javascript|duktape|exception|failed|critical'
LOG_FILTER='level:WARNING'
VERBOSE=0
PRINT_ALL=0
JOTTER="${ROOT}/jotter"
SLATE_OPTIONS=("--enable_javascript=1")

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] URL [-- extra-slate-options...]

Dry-run a URL through the local jotter frontend with JavaScript enabled.
The full output is written to a log file, while the terminal shows only
warning/error/JavaScript-like lines by default.

Options:
  -w, --wait SECONDS       Time to allow the page to load and run JS (default: ${WAIT_SECONDS})
  -o, --log FILE           Full output log path (default: ${LOG_FILE})
  -f, --filter REGEX       Case-insensitive terminal filter regex (default: ${ERROR_FILTER})
  -a, --all                Print all output to the terminal
  -v, --verbose            Use Slate verbose logging at INFO level
  -j, --jotter PATH        jotter binary to run (default: ${JOTTER})
  -h, --help               Show this help

Examples:
  scripts/run-js.sh https://openstreetmap.org/
  scripts/run-js.sh --wait 60 --log /tmp/osm.log https://openstreetmap.org/
  scripts/run-js.sh -v https://example.com/ -- --max_fetchers=4
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

URL=""
while [ "$#" -gt 0 ]; do
	case "$1" in
		-w|--wait)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			WAIT_SECONDS="$2"
			shift 2
			;;
		-o|--log)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			LOG_FILE="$2"
			shift 2
			;;
		-f|--filter)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			ERROR_FILTER="$2"
			shift 2
			;;
		-a|--all)
			PRINT_ALL=1
			shift
			;;
		-v|--verbose)
			VERBOSE=1
			LOG_FILTER='level:INFO'
			shift
			;;
		-j|--jotter)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			JOTTER="$2"
			shift 2
			;;
		-h|--help)
			usage
			exit 0
			;;
		--)
			shift
			while [ "$#" -gt 0 ]; do
				SLATE_OPTIONS+=("$1")
				shift
			done
			;;
		-*)
			die "Unknown option: $1"
			;;
		*)
			[ -z "${URL}" ] || die "Only one URL can be supplied"
			URL="$1"
			shift
			;;
	esac
done

[ -n "${URL}" ] || {
	usage >&2
	exit 2
}

[[ "${WAIT_SECONDS}" =~ ^[0-9]+$ ]] || die "--wait must be a whole number of seconds"
[ -x "${JOTTER}" ] || die "jotter is not executable: ${JOTTER}"

JOTTER_ARGS=("--log_filter=${LOG_FILTER}")
if [ "${VERBOSE}" -eq 1 ]; then
	JOTTER_ARGS=("-v" "--verbose_filter=${LOG_FILTER}")
fi

printf 'Dry-running %s with %s for %ss\n' "${URL}" "${JOTTER}" "${WAIT_SECONDS}" >&2
printf 'Full log: %s\n' "${LOG_FILE}" >&2

{
	printf 'OPTIONS'
	for option in "${SLATE_OPTIONS[@]}"; do
		printf ' %s' "${option}"
	done
	printf '\n'
	printf 'WINDOW NEW %s\n' "${URL}"
	sleep "${WAIT_SECONDS}"
	printf 'QUIT\n'
} | "${JOTTER}" "${JOTTER_ARGS[@]}" 2>&1 \
	| tee "${LOG_FILE}" \
	| awk -v print_all="${PRINT_ALL}" -v filter="${ERROR_FILTER}" \
		'print_all || tolower($0) ~ tolower(filter) { print; fflush(); }'
