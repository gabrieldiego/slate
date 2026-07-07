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
