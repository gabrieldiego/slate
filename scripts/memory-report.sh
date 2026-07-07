#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

RUN_ID="$(date -u +%Y%m%d-%H%M%S)"
REPORT=""
MONKEY="${ROOT}/nsmonkey"
TIME_BIN="/usr/bin/time"
BENCH_ROOT="file://${ROOT}/test/bench/pages"
SUITES=(smoke practical stress)

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] [suite...]

Generate a Markdown memory report for the local Monkey benchmark suites.
Suites default to: ${SUITES[*]}

Options:
  -o, --output FILE        Markdown report path
                           default: build/bench/<timestamp>/memory-report.md
  -m, --monkey PATH        nsmonkey binary to run
                           default: ${MONKEY}
  -h, --help               Show this help

Examples:
  scripts/memory-report.sh
  scripts/memory-report.sh smoke practical
  scripts/memory-report.sh -o build/bench/baseline.md
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

while [ "$#" -gt 0 ]; do
	case "$1" in
		-o|--output)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			REPORT="$2"
			shift 2
			;;
		-m|--monkey)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			MONKEY="$2"
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
			SUITES=("$@")
			break
			;;
	esac
done

if [ -z "${REPORT}" ]; then
	REPORT="${ROOT}/build/bench/${RUN_ID}/memory-report.md"
fi

REPORT_DIR="$(cd "$(dirname "${REPORT}")" 2>/dev/null && pwd)"
if [ -z "${REPORT_DIR}" ]; then
	REPORT_DIR="$(dirname "${REPORT}")"
	mkdir -p "${REPORT_DIR}" || exit 1
	REPORT_DIR="$(cd "${REPORT_DIR}" && pwd)"
else
	mkdir -p "${REPORT_DIR}" || exit 1
fi
REPORT="${REPORT_DIR}/$(basename "${REPORT}")"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh" >/dev/null || exit 1

[ -x "${MONKEY}" ] || die "Missing executable nsmonkey: ${MONKEY}"
[ -x "${TIME_BIN}" ] || die "Missing GNU time binary: ${TIME_BIN}"

metric() {
	local key="$1"
	local file="$2"
	awk -v key="${key}" '
		BEGIN {
			prefix = key ":"
		}
		{
			line = $0
			sub(/^[ \t]+/, "", line)
		}
		index(line, prefix) == 1 {
			value = substr(line, length(prefix) + 1)
			sub(/^[ \t]+/, "", value)
			print value
			exit
		}
	' "${file}" 2>/dev/null
}

kib_to_mib() {
	local kib="$1"
	if [ -z "${kib}" ]; then
		printf ''
	else
		awk -v kib="${kib}" 'BEGIN { printf "%.1f", kib / 1024 }'
	fi
}

git_rev="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || printf unknown)"
git_branch="$(git -C "${ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null || printf unknown)"
if [ -n "$(git -C "${ROOT}" status --short 2>/dev/null)" ]; then
	git_dirty="yes"
else
	git_dirty="no"
fi

{
	printf '# NetSurf Memory Benchmark Report\n\n'
	printf -- '- Generated: `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
	printf -- '- Git revision: `%s` on `%s`\n' "${git_rev}" "${git_branch}"
	printf -- '- Git dirty: `%s`\n' "${git_dirty}"
	printf -- '- Monkey binary: `%s`\n' "${MONKEY}"
	printf -- '- Bench root: `%s`\n' "${BENCH_ROOT}"
	printf -- '- Time tool: `%s -v`\n\n' "${TIME_BIN}"
	printf '## Summary\n\n'
	printf '| Suite | Result | Max RSS KiB | Max RSS MiB | Elapsed | User CPU | System CPU | Exit | Raw files |\n'
	printf '| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | --- |\n'
} > "${REPORT}"

overall_status=0

for suite in "${SUITES[@]}"; do
	case "${suite}" in
		smoke|practical|stress)
			;;
		*)
			die "Unknown benchmark suite: ${suite}"
			;;
	esac

	test_file="${ROOT}/test/bench/monkey-tests/${suite}.yaml"
	log_file="${REPORT_DIR}/${suite}.log"
	time_file="${REPORT_DIR}/${suite}.time"

	printf 'Running %s...\n' "${suite}" >&2

	NETSURF_BENCH_ROOT="${BENCH_ROOT}" \
		"${ROOT}/test/monkey_driver.py" \
		-m "${MONKEY}" \
		-w "${TIME_BIN} -v -o ${time_file}" \
		-t "${test_file}" >"${log_file}" 2>&1
	status=$?

	if [ "${status}" -eq 0 ]; then
		result="pass"
	else
		result="fail"
		overall_status=1
	fi

	max_rss="$(metric 'Maximum resident set size (kbytes)' "${time_file}")"
	elapsed="$(metric 'Elapsed (wall clock) time (h:mm:ss or m:ss)' "${time_file}")"
	user_cpu="$(metric 'User time (seconds)' "${time_file}")"
	sys_cpu="$(metric 'System time (seconds)' "${time_file}")"
	time_exit="$(metric 'Exit status' "${time_file}")"
	max_rss_mib="$(kib_to_mib "${max_rss}")"

	{
		printf '| `%s` | %s | %s | %s | `%s` | %s | %s | %s | [log](%s), [time](%s) |\n' \
			"${suite}" \
			"${result}" \
			"${max_rss:-}" \
			"${max_rss_mib:-}" \
			"${elapsed:-}" \
			"${user_cpu:-}" \
			"${sys_cpu:-}" \
			"${time_exit:-${status}}" \
			"$(basename "${log_file}")" \
			"$(basename "${time_file}")"
	} >> "${REPORT}"
done

{
	printf '\n## Notes\n\n'
	printf -- '- `Max RSS` is reported by GNU `/usr/bin/time -v` for the `nsmonkey` process.\n'
	printf -- '- A failed suite still keeps its `.log` and `.time` files next to this report.\n'
	printf -- '- Use this report as a baseline trend signal; compare runs made from similar build options and host conditions.\n'
} >> "${REPORT}"

printf 'Wrote %s\n' "${REPORT}" >&2
exit "${overall_status}"
