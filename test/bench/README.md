# NetSurf local benchmark suites

These Monkey suites use checked-in local HTML pages so memory measurements are
repeatable and do not depend on the network.

The suites intentionally keep three runtime profiles:

* `smoke`: tiny parser, CSS, layout, and shutdown checks.
* `practical`: ordinary browsing-shaped pages with articles, forms, component
  layouts, embedded content, and one DOM event interaction.
* `stress`: synthetic high-pressure pages for DOM size, CSS selectors, tables,
  flex, forms, objects, image maps, and inline layout.

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

Generate Markdown memory and/or HTML renderer coverage reports:

```sh
scripts/bench-report.sh --mode both
scripts/bench-report.sh --mode memory
scripts/bench-report.sh --mode coverage
```

Reports are written under `build/bench/<timestamp>/` by default, alongside the
raw Monkey and `/usr/bin/time -v` logs for each suite. Treat the numbers as a
baseline trend signal rather than a hard pass/fail threshold.

Coverage mode rebuilds the Monkey frontend with GCC coverage instrumentation, runs the
benchmark suites, and tallies only `content/handlers/html/*.c`. It only needs
`gcov` from GCC; `lcov` and `gcovr` are not required for this report.

Coverage builds are kept separate from the normal Monkey binary:

* normal objects: `build/<host>-monkey/`
* coverage objects: `build/<host>-monkey-cov/`
* normal binary: `nsmonkey`
* coverage binary: `nsmonkey-cov`

Use `scripts/bench-report.sh --mode coverage --clean` only when you want to
force a fresh coverage rebuild.
