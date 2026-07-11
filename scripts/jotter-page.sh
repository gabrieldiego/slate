#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

JOTTER="${ROOT}/jotter"
TIMEOUT_SECS=30
WAITS=()
PAGE=""

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] PAGE

Run a temporary one-page Jotter plan for quick rendering and JS diagnostics.
PAGE may be a file URL, http(s) URL, absolute path, or repo-relative path.

Options:
  -m, --jotter PATH     Jotter binary to run
                        default: ${JOTTER}
  --timeout SECONDS     Kill the driver after this many seconds
                        default: ${TIMEOUT_SECS}; use 0 to disable
  --wait TEXT           Wait for a console log substring
                        may be repeated
  -h, --help            Show this help
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

yaml_quote() {
	printf "'%s'" "$(printf '%s' "$1" | sed "s/'/''/g")"
}

page_to_url() {
	case "$1" in
		file://*|http://*|https://*)
			printf '%s' "$1"
			;;
		/*)
			printf 'file://%s' "$1"
			;;
		*)
			printf 'file://%s/%s' "${ROOT}" "$1"
			;;
	esac
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
		--wait)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			WAITS+=("$2")
			shift 2
			;;
		--wait=*)
			WAITS+=("${1#*=}")
			shift
			;;
		-h|--help)
			usage
			exit 0
			;;
		-*)
			die "Unknown option: $1"
			;;
		*)
			[ -z "${PAGE}" ] || die "Only one page may be supplied"
			PAGE="$1"
			shift
			;;
	esac
done

[ -n "${PAGE}" ] || die "Missing page"

PLAN="$(mktemp "${TMPDIR:-/tmp}/slate-jotter-page.XXXXXX.yaml")" || exit 1
trap 'rm -f "${PLAN}"' EXIT

URL="$(page_to_url "${PAGE}")"

{
	printf '%s\n' "title: jotter page"
	printf '%s\n' "group: jotter-page"
	printf '%s\n' "steps:"
	printf '%s\n' "- action: launch"
	printf '%s\n' "  language: en"
	printf '%s\n' "  options:"
	printf '%s\n' "  - enable_javascript=1"
	printf '%s\n' "  - memory_cache_size=4194304"
	printf '%s\n' "  - disc_cache_size=0"
	printf '%s\n' "- action: window-new"
	printf '%s\n' "  tag: win1"
	printf '%s\n' "- action: navigate"
	printf '%s\n' "  window: win1"
	printf '  url: '
	yaml_quote "${URL}"
	printf '\n'
	printf '%s\n' "- action: block"
	printf '%s\n' "  conditions:"
	printf '%s\n' "  - window: win1"
	printf '%s\n' "    status: complete"
	for wait in "${WAITS[@]}"; do
		printf '%s\n' "- action: wait-log"
		printf '%s\n' "  window: win1"
		printf '  substring: '
		yaml_quote "${wait}"
		printf '\n'
	done
	printf '%s\n' "- action: quit"
} >"${PLAN}"

"${ROOT}/scripts/jotter-run.sh" \
	--jotter "${JOTTER}" \
	--timeout "${TIMEOUT_SECS}" \
	"${PLAN}"
