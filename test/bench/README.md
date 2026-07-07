# NetSurf local benchmark suites

These Monkey suites use checked-in local HTML pages so memory measurements are
repeatable and do not depend on the network.

Run one suite:

```sh
make TARGET=monkey bench-smoke
make TARGET=monkey bench-practical
make TARGET=monkey bench-stress
```

Run all three:

```sh
make TARGET=monkey bench
```

By default the Monkey process is wrapped with `/usr/bin/time -v`, which reports
maximum resident set size. Override `BENCH_WRAPPER` to use Massif, Heaptrack, or
no wrapper.

Generate a Markdown memory report:

```sh
scripts/memory-report.sh
```

Reports are written under `build/bench/<timestamp>/` by default, alongside the
raw Monkey and `/usr/bin/time -v` logs for each suite.

Generate a Markdown line-coverage report for the HTML renderer sources:

```sh
scripts/html-coverage-report.sh
```

This rebuilds the Monkey frontend with GCC coverage instrumentation, runs the
benchmark suites, and tallies only `content/handlers/html/*.c`. It only needs
`gcov` from GCC; `lcov` and `gcovr` are not required for this report.
