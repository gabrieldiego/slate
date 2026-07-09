# Slate local benchmark suites

These Jotter suites use checked-in local HTML pages so memory measurements are
repeatable and do not depend on the network.

The suites intentionally keep three runtime profiles:

* `smoke`: tiny parser, CSS, JavaScript, layout, and shutdown checks.
* `practical`: ordinary browsing-shaped pages with articles, forms, component
  layouts, embedded content, JavaScript DOM updates, and DOM event interactions.
* `stress`: synthetic high-pressure pages for DOM size, CSS selectors, tables,
  flex, forms, objects, image maps, inline layout, and JavaScript-generated DOM.

The practical slippy-map page also logs map-site compatibility gaps such as
selector forms, computed CSS properties, abort signals, and observer APIs.  Each
compatibility fix should add or tighten checks in one of these existing suites
instead of adding a network-dependent page.

Run one suite:

```sh
make TARGET=jotter bench-smoke
make TARGET=jotter bench-practical
make TARGET=jotter bench-stress
```

Run all three:

```sh
make TARGET=jotter bench
```

By default the Jotter process is wrapped with `/usr/bin/time -v`, which reports
maximum resident set size. Override `BENCH_WRAPPER` to use Massif, Heaptrack, or
no wrapper.

Generate Markdown memory and/or engine coverage reports:

```sh
scripts/bench-report.sh --mode both
scripts/bench-report.sh --mode memory
scripts/bench-report.sh --mode coverage
```

Reports are written under `build/bench/<timestamp>/` by default, alongside the
raw Jotter and `/usr/bin/time -v` logs for each suite. Treat the numbers as a
baseline trend signal rather than a hard pass/fail threshold.

Coverage mode rebuilds the Jotter frontend with GCC coverage instrumentation,
runs the benchmark suites, and tallies the HTML renderer and QuickJS JavaScript
engine separately. It only needs `gcov` from GCC; `lcov` and `gcovr` are not
required for this report.

Coverage builds are kept separate from the normal Jotter binary:

* normal objects: `build/<host>-jotter/`
* coverage objects: `build/<host>-jotter-cov/`
* normal binary: `jotter`
* coverage binary: `jotter-cov`

Use `scripts/bench-report.sh --mode coverage --clean` only when you want to
force a fresh coverage rebuild.
