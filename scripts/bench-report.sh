#!/usr/bin/env bash
set -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

RUN_ID="$(date -u +%Y%m%d-%H%M%S)"
MODE="both"
OUTPUT_DIR=""
JOTTER="${ROOT}/jotter"
TIME_BIN="/usr/bin/time"
GCOV_BIN="${GCOV:-gcov}"
BENCH_ROOT="file://${ROOT}/test/bench/pages"
SUITES=(smoke practical stress)

COVERAGE_SUBTARGET="-cov"
COVERAGE_EXETARGET="jotter-cov"
COVERAGE_CFLAGS="-O0 -g --coverage"
COVERAGE_LDFLAGS="--coverage"
CLEAN_COVERAGE=0

usage() {
	cat <<EOF
Usage:
  ${0##*/} [options] [suite...]

Generate Markdown reports for the local Jotter benchmark suites.
Suites default to: ${SUITES[*]}

Options:
  --mode MODE          Report mode: memory, coverage, or both
                       default: ${MODE}
  -o, --output DIR     Report directory
                       default: build/bench/<timestamp>/
  -j, --jotter PATH    Normal jotter binary for memory runs
                       default: ${JOTTER}
  --clean              Rebuild coverage objects from scratch
  -h, --help           Show this help

Examples:
  scripts/bench-report.sh
  scripts/bench-report.sh --mode memory smoke practical
  scripts/bench-report.sh --mode coverage --clean
EOF
}

die() {
	printf '%s\n' "$*" >&2
	exit 2
}

validate_suite() {
	case "$1" in
		smoke|practical|stress)
			;;
		*)
			die "Unknown benchmark suite: $1"
			;;
	esac
}

while [ "$#" -gt 0 ]; do
	case "$1" in
		--mode)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			MODE="$2"
			shift 2
			;;
		--mode=*)
			MODE="${1#*=}"
			shift
			;;
		-o|--output)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			OUTPUT_DIR="$2"
			shift 2
			;;
		-j|--jotter)
			[ "$#" -ge 2 ] || die "Missing value for $1"
			JOTTER="$2"
			shift 2
			;;
		--clean)
			CLEAN_COVERAGE=1
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
			SUITES=("$@")
			break
			;;
	esac
done

case "${MODE}" in
	memory|coverage|both)
		;;
	*)
		die "Unknown mode: ${MODE}"
		;;
esac

for suite in "${SUITES[@]}"; do
	validate_suite "${suite}"
done

if [ -z "${OUTPUT_DIR}" ]; then
	OUTPUT_DIR="${ROOT}/build/bench/${RUN_ID}"
fi

mkdir -p "${OUTPUT_DIR}" || exit 1
OUTPUT_DIR="$(cd "${OUTPUT_DIR}" && pwd)"

# shellcheck source=/dev/null
source "${ROOT}/scripts/local-env.sh" >/dev/null || exit 1
set +e
set -o pipefail

git_rev="$(git -C "${ROOT}" rev-parse --short HEAD 2>/dev/null || printf unknown)"
git_branch="$(git -C "${ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null || printf unknown)"
if [ -n "$(git -C "${ROOT}" status --short 2>/dev/null)" ]; then
	git_dirty="yes"
else
	git_dirty="no"
fi

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

host_name() {
	local host
	host="$(uname -s)"
	host="${host//./_}"
	host="${host//-/_}"
	host="${host//\//_}"
	case "${host}" in
		Haiku)
			host="beos"
			;;
		FreeMiNT)
			host="mint"
			;;
		OpenBSD)
			host="openbsd"
			;;
		MINGW*)
			host="windows"
			;;
	esac
	printf '%s\n' "${host}"
}

run_memory_report() {
	local report="$1"
	local overall_status=0

	[ -x "${JOTTER}" ] || die "Missing executable jotter: ${JOTTER}"
	[ -x "${TIME_BIN}" ] || die "Missing GNU time binary: ${TIME_BIN}"

	{
		printf '# Slate Memory Benchmark Report\n\n'
		printf -- '- Generated: `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
		printf -- '- Git revision: `%s` on `%s`\n' "${git_rev}" "${git_branch}"
		printf -- '- Git dirty: `%s`\n' "${git_dirty}"
		printf -- '- Jotter binary: `%s`\n' "${JOTTER}"
		printf -- '- Bench root: `%s`\n' "${BENCH_ROOT}"
		printf -- '- Time tool: `%s -v`\n\n' "${TIME_BIN}"
		printf '## Summary\n\n'
		printf '| Suite | Result | Max RSS KiB | Max RSS MiB | Elapsed | User CPU | System CPU | Exit | Raw files |\n'
		printf '| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | --- |\n'
	} > "${report}"

	for suite in "${SUITES[@]}"; do
		local test_file="${ROOT}/test/bench/monkey-tests/${suite}.yaml"
		local log_file="${OUTPUT_DIR}/memory-${suite}.log"
		local time_file="${OUTPUT_DIR}/memory-${suite}.time"
		local status result max_rss elapsed user_cpu sys_cpu time_exit max_rss_mib

		printf 'Running memory %s...\n' "${suite}" >&2
		SLATE_BENCH_ROOT="${BENCH_ROOT}" \
			"${ROOT}/test/monkey_driver.py" \
			-m "${JOTTER}" \
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
			"$(basename "${time_file}")" >> "${report}"
	done

	{
		printf '\n## Notes\n\n'
		printf -- '- `Max RSS` is reported by GNU `/usr/bin/time -v` for the `jotter` process.\n'
		printf -- '- A failed suite still keeps its `.log` and `.time` files next to this report.\n'
		printf -- '- Use this report as a baseline trend signal; compare runs made from similar build options and host conditions.\n'
	} >> "${report}"

	printf 'Wrote %s\n' "${report}" >&2
	return "${overall_status}"
}

run_coverage_report() {
	local report="$1"
	local host objroot coverage_jotter build_log run_summary coverage_tsv gcov_dir
	local overall_status=0
	local html_sources=()
	local total_covered=0
	local total_lines=0
	local total_percent

	command -v "${GCOV_BIN}" >/dev/null 2>&1 || die "Missing gcov binary: ${GCOV_BIN}"

	host="$(host_name)"
	objroot="${ROOT}/build/${host}-monkey${COVERAGE_SUBTARGET}"
	coverage_jotter="${ROOT}/${COVERAGE_EXETARGET}"
	build_log="${OUTPUT_DIR}/coverage-build-jotter.log"
	run_summary="${OUTPUT_DIR}/coverage-suite-summary.tsv"
	coverage_tsv="${OUTPUT_DIR}/html-coverage.tsv"
	gcov_dir="${OUTPUT_DIR}/gcov"
	mkdir -p "${gcov_dir}" || return 1

	if [ "${CLEAN_COVERAGE}" -eq 1 ]; then
		printf 'Cleaning coverage jotter build...\n' >&2
		make -C "${ROOT}" \
			TARGET=jotter \
			SUBTARGET="${COVERAGE_SUBTARGET}" \
			EXETARGET="${COVERAGE_EXETARGET}" \
			clean >"${build_log}" 2>&1
		clean_status=$?
		if [ "${clean_status}" -ne 0 ]; then
			printf 'Coverage jotter clean failed; see %s\n' "${build_log}" >&2
			return "${clean_status}"
		fi
	else
		: > "${build_log}"
	fi

	printf 'Building jotter with coverage flags...\n' >&2
	CFLAGS="${COVERAGE_CFLAGS}" \
	CXXFLAGS="${COVERAGE_CFLAGS}" \
	LDFLAGS="${COVERAGE_LDFLAGS}" \
	make -C "${ROOT}" ${USE_CPUS} \
		TARGET=jotter \
		SUBTARGET="${COVERAGE_SUBTARGET}" \
		EXETARGET="${COVERAGE_EXETARGET}" >>"${build_log}" 2>&1
	build_status=$?
	if [ "${build_status}" -ne 0 ]; then
		printf 'Coverage build failed; see %s\n' "${build_log}" >&2
		return "${build_status}"
	fi

	find "${objroot}" -name '*.gcda' -delete
	: > "${run_summary}"

	for suite in "${SUITES[@]}"; do
		local test_file="${ROOT}/test/bench/monkey-tests/${suite}.yaml"
		local log_file="${OUTPUT_DIR}/coverage-${suite}.log"
		local status result

		printf 'Running coverage %s...\n' "${suite}" >&2
		SLATE_BENCH_ROOT="${BENCH_ROOT}" \
			"${ROOT}/test/monkey_driver.py" \
			-m "${coverage_jotter}" \
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

	while IFS= read -r source_file; do
		html_sources+=("${source_file}")
	done < <(find "${ROOT}/content/handlers/html" -maxdepth 1 -name '*.c' -printf '%P\n' | sort)

	: > "${coverage_tsv}"

	printf 'Collecting HTML renderer coverage...\n' >&2
	for source_name in "${html_sources[@]}"; do
		local source_rel="content/handlers/html/${source_name}"
		local object_name="${source_rel%.c}"
		local object_path gcov_file gcov_log gcov_status covered lines percent
		object_name="${object_name//\//_}.o"
		object_path="${objroot}/${object_name}"
		gcov_file="${gcov_dir}/${source_name}.gcov"
		gcov_log="${gcov_dir}/${source_name}.gcov.log"

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

	{
		printf '# Slate HTML Renderer Coverage Report\n\n'
		printf -- '- Generated: `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
		printf -- '- Git revision: `%s` on `%s`\n' "${git_rev}" "${git_branch}"
		printf -- '- Git dirty: `%s`\n' "${git_dirty}"
		printf -- '- Target: `jotter`\n'
		printf -- '- Coverage binary: `%s`\n' "${coverage_jotter}"
		printf -- '- Coverage object root: `%s`\n' "${objroot}"
		printf -- '- Coverage flags: `%s`\n' "${COVERAGE_CFLAGS}"
		printf -- '- Bench root: `%s`\n' "${BENCH_ROOT}"
		printf -- '- Scope: `content/handlers/html/*.c`\n'
		printf -- '- Raw build log: [coverage-build-jotter.log](%s)\n\n' "$(basename "${build_log}")"

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
		printf -- '- Coverage objects are kept separately using `SUBTARGET=%s` on the internal monkey target.\n' "${COVERAGE_SUBTARGET}"
		printf -- '- The instrumented binary is `%s`; the normal `jotter` binary is not overwritten.\n' "${COVERAGE_EXETARGET}"
		printf -- '- Platform-specific and frontend-specific browser code is intentionally outside this tally.\n'
	} > "${report}"

	printf 'Wrote %s\n' "${report}" >&2
	return "${overall_status}"
}

write_index_report() {
	local report="${OUTPUT_DIR}/bench-report.md"
	{
		printf '# Slate Benchmark Report\n\n'
		printf -- '- Generated: `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
		printf -- '- Mode: `%s`\n' "${MODE}"
		printf -- '- Suites: `%s`\n' "${SUITES[*]}"
		printf -- '- Git revision: `%s` on `%s`\n' "${git_rev}" "${git_branch}"
		printf -- '- Git dirty: `%s`\n\n' "${git_dirty}"
		printf '## Reports\n\n'
		if [ -f "${OUTPUT_DIR}/memory-report.md" ]; then
			printf -- '- [Memory report](memory-report.md)\n'
		fi
		if [ -f "${OUTPUT_DIR}/html-coverage-report.md" ]; then
			printf -- '- [HTML renderer coverage report](html-coverage-report.md)\n'
		fi
	} > "${report}"
	printf 'Wrote %s\n' "${report}" >&2
}

overall_status=0

case "${MODE}" in
	memory)
		run_memory_report "${OUTPUT_DIR}/memory-report.md"
		overall_status=$?
		;;
	coverage)
		run_coverage_report "${OUTPUT_DIR}/html-coverage-report.md"
		overall_status=$?
		;;
	both)
		run_memory_report "${OUTPUT_DIR}/memory-report.md"
		memory_status=$?
		run_coverage_report "${OUTPUT_DIR}/html-coverage-report.md"
		coverage_status=$?
		write_index_report
		if [ "${memory_status}" -ne 0 ] || [ "${coverage_status}" -ne 0 ]; then
			overall_status=1
		fi
		;;
esac

exit "${overall_status}"
