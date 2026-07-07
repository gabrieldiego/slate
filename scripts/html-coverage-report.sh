#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

RUN_ID="$(date -u +%Y%m%d-%H%M%S)"
REPORT=""
GCOV_BIN="${GCOV:-gcov}"
BENCH_ROOT="file://${ROOT}/test/bench/pages"
SUITES=(smoke practical stress)
COVERAGE_SUBTARGET="-cov"
COVERAGE_EXETARGET="nsmonkey-cov"
COVERAGE_CFLAGS="-O0 -g --coverage"
COVERAGE_LDFLAGS="--coverage"
CLEAN_COVERAGE=0

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] [suite...]

Build nsmonkey with GCC coverage instrumentation, run the local Monkey
benchmark suites, and generate a Markdown line-coverage report for only:

  content/handlers/html/*.c

Suites default to: ${SUITES[*]}

Options:
  -o, --output FILE        Markdown report path
                           default: build/bench/<timestamp>/html-coverage-report.md
  --clean                  Rebuild coverage objects from scratch
  -h, --help               Show this help

Examples:
  scripts/html-coverage-report.sh
  scripts/html-coverage-report.sh smoke practical
  scripts/html-coverage-report.sh -o build/bench/html-baseline.md
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
		-h|--help)
			usage
			exit 0
			;;
		--clean)
			CLEAN_COVERAGE=1
			shift
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
	REPORT="${ROOT}/build/bench/${RUN_ID}/html-coverage-report.md"
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
GCOV_DIR="${REPORT_DIR}/gcov"
mkdir -p "${GCOV_DIR}" || exit 1

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh" >/dev/null || exit 1

command -v "${GCOV_BIN}" >/dev/null 2>&1 || die "Missing gcov binary: ${GCOV_BIN}"

HOST="$(uname -s)"
HOST="${HOST//./_}"
HOST="${HOST//-/_}"
HOST="${HOST//\//_}"
case "${HOST}" in
	Haiku)
		HOST="beos"
		;;
	FreeMiNT)
		HOST="mint"
		;;
	OpenBSD)
		HOST="openbsd"
		;;
	MINGW*)
		HOST="windows"
		;;
esac

OBJROOT="${ROOT}/build/${HOST}-monkey${COVERAGE_SUBTARGET}"
COVERAGE_MONKEY="${ROOT}/${COVERAGE_EXETARGET}"
BUILD_LOG="${REPORT_DIR}/build-monkey-coverage.log"

if [ "${CLEAN_COVERAGE}" -eq 1 ]; then
	printf 'Cleaning coverage monkey build...\n' >&2
	make -C "${ROOT}" \
		TARGET=monkey \
		SUBTARGET="${COVERAGE_SUBTARGET}" \
		EXETARGET="${COVERAGE_EXETARGET}" \
		clean >"${BUILD_LOG}" 2>&1
	clean_status=$?
	if [ "${clean_status}" -ne 0 ]; then
		printf 'Coverage monkey clean failed; see %s\n' "${BUILD_LOG}" >&2
		exit "${clean_status}"
	fi
else
	: > "${BUILD_LOG}"
fi

printf 'Building monkey with coverage flags...\n' >&2
CFLAGS="${COVERAGE_CFLAGS}" \
CXXFLAGS="${COVERAGE_CFLAGS}" \
LDFLAGS="${COVERAGE_LDFLAGS}" \
make -C "${ROOT}" ${USE_CPUS} \
	TARGET=monkey \
	SUBTARGET="${COVERAGE_SUBTARGET}" \
	EXETARGET="${COVERAGE_EXETARGET}" >>"${BUILD_LOG}" 2>&1
build_status=$?
if [ "${build_status}" -ne 0 ]; then
	printf 'Coverage build failed; see %s\n' "${BUILD_LOG}" >&2
	exit "${build_status}"
fi

find "${OBJROOT}" -name '*.gcda' -delete

overall_status=0
run_summary="${REPORT_DIR}/suite-summary.tsv"
: > "${run_summary}"

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

	printf 'Running %s...\n' "${suite}" >&2
	NETSURF_BENCH_ROOT="${BENCH_ROOT}" \
		"${ROOT}/test/monkey_driver.py" \
		-m "${COVERAGE_MONKEY}" \
		-t "${test_file}" >"${log_file}" 2>&1
	status=$?

	if [ "${status}" -eq 0 ]; then
		result="pass"
	else
		result="fail"
		overall_status=1
	fi

	printf '%s\t%s\t%s\t%s\n' "${suite}" "${result}" "${status}" "$(basename "${log_file}")" >> "${run_summary}"
done

html_sources=()
while IFS= read -r source_file; do
	html_sources+=("${source_file}")
done < <(find "${ROOT}/content/handlers/html" -maxdepth 1 -name '*.c' -printf '%P\n' | sort)

coverage_tsv="${REPORT_DIR}/html-coverage.tsv"
: > "${coverage_tsv}"

total_covered=0
total_lines=0

printf 'Collecting HTML renderer coverage...\n' >&2
for source_name in "${html_sources[@]}"; do
	source_rel="content/handlers/html/${source_name}"
	object_name="${source_rel%.c}"
	object_name="${object_name//\//_}.o"
	object_path="${OBJROOT}/${object_name}"
	gcov_file="${GCOV_DIR}/${source_name}.gcov"
	gcov_log="${GCOV_DIR}/${source_name}.gcov.log"

	if [ ! -f "${object_path%.o}.gcno" ]; then
		printf '%s\t0\t0\t0.0\tmissing-gcno\t%s\n' "${source_rel}" "${source_name}.gcov.log" >> "${coverage_tsv}"
		continue
	fi

	(
		cd "${ROOT}" || exit 1
		"${GCOV_BIN}" -t -b -c -o "${object_path}" "${source_rel}"
	) >"${gcov_file}" 2>"${gcov_log}"
	gcov_status=$?

	if [ "${gcov_status}" -ne 0 ]; then
		printf '%s\t0\t0\t0.0\tgcov-failed\t%s\n' "${source_rel}" "${source_name}.gcov.log" >> "${coverage_tsv}"
		continue
	fi

	read -r covered lines < <(
		awk -F: -v wanted="${source_rel}" '
			$1 ~ /^[ \t]*-/ && $2 ~ /^[ \t]*0$/ && $3 == "Source" {
				active = ($4 == wanted)
				next
			}
			active && NF >= 3 {
				count = $1
				gsub(/^[ \t]+|[ \t]+$/, "", count)
				if (count != "-" && count != "") {
					lines++
					if (count ~ /^[0-9]+/ && count + 0 > 0) {
						covered++
					}
				}
			}
			END {
				print covered + 0, lines + 0
			}
		' "${gcov_file}"
	)

	if [ "${lines}" -gt 0 ]; then
		percent="$(awk -v covered="${covered}" -v lines="${lines}" 'BEGIN { printf "%.1f", covered * 100 / lines }')"
	else
		percent="0.0"
	fi

	total_covered=$((total_covered + covered))
	total_lines=$((total_lines + lines))
	printf '%s\t%s\t%s\t%s\tok\t%s\n' "${source_rel}" "${covered}" "${lines}" "${percent}" "${source_name}.gcov" >> "${coverage_tsv}"
done

if [ "${total_lines}" -gt 0 ]; then
	total_percent="$(awk -v covered="${total_covered}" -v lines="${total_lines}" 'BEGIN { printf "%.1f", covered * 100 / lines }')"
else
	total_percent="0.0"
fi

git_rev="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || printf unknown)"
git_branch="$(git -C "${ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null || printf unknown)"
if [ -n "$(git -C "${ROOT}" status --short 2>/dev/null)" ]; then
	git_dirty="yes"
else
	git_dirty="no"
fi

{
	printf '# NetSurf HTML Renderer Coverage Report\n\n'
	printf -- '- Generated: `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
	printf -- '- Git revision: `%s` on `%s`\n' "${git_rev}" "${git_branch}"
	printf -- '- Git dirty: `%s`\n' "${git_dirty}"
	printf -- '- Target: `monkey`\n'
	printf -- '- Coverage binary: `%s`\n' "${COVERAGE_MONKEY}"
	printf -- '- Coverage object root: `%s`\n' "${OBJROOT}"
	printf -- '- Coverage flags: `%s`\n' "${COVERAGE_CFLAGS}"
	printf -- '- Bench root: `%s`\n' "${BENCH_ROOT}"
	printf -- '- Scope: `content/handlers/html/*.c`\n'
	printf -- '- Raw build log: [build-monkey-coverage.log](%s)\n\n' "$(basename "${BUILD_LOG}")"

	printf '## Summary\n\n'
	printf '| Covered lines | Instrumented lines | Line coverage |\n'
	printf '| ---: | ---: | ---: |\n'
	printf '| %s | %s | %s%% |\n\n' "${total_covered}" "${total_lines}" "${total_percent}"

	printf '## Benchmark Runs\n\n'
	printf '| Suite | Result | Exit | Log |\n'
	printf '| --- | --- | ---: | --- |\n'
	while IFS=$'\t' read -r suite result status log_name; do
		printf '| `%s` | %s | %s | [log](%s) |\n' "${suite}" "${result}" "${status}" "${log_name}"
	done < "${run_summary}"

	printf '\n## HTML Renderer Files\n\n'
	printf '| Source | Covered | Lines | Coverage | Status | Raw |\n'
	printf '| --- | ---: | ---: | ---: | --- | --- |\n'
	while IFS=$'\t' read -r source_rel covered lines percent status raw_name; do
		if [ "${status}" = "ok" ]; then
			raw_link="[$(basename "${raw_name}")](gcov/${raw_name})"
		else
			raw_link="[log](gcov/${raw_name})"
		fi
		printf '| `%s` | %s | %s | %s%% | %s | %s |\n' \
			"${source_rel}" "${covered}" "${lines}" "${percent}" "${status}" "${raw_link}"
	done < "${coverage_tsv}"

	printf '\n## Notes\n\n'
	printf -- '- This report only tallies line coverage for `content/handlers/html/*.c`.\n'
	printf -- '- Coverage objects are kept separately using `SUBTARGET=%s`.\n' "${COVERAGE_SUBTARGET}"
	printf -- '- The instrumented binary is `%s`; the normal `nsmonkey` binary is not overwritten.\n' "${COVERAGE_EXETARGET}"
	printf -- '- Platform-specific and frontend-specific browser code is intentionally outside this tally.\n'
} > "${REPORT}"

printf 'Wrote %s\n' "${REPORT}" >&2
exit "${overall_status}"
