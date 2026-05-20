# Local NetSurf Build

This checkout is set up so the NetSurf browser, support repositories, install
prefix, and generated build output stay under this directory.

## Layout

- `projects/`: cloned NetSurf support libraries and tools.
- `projects/inst-*`: local install prefixes used by NetSurf's build system.
- `build/`: browser build output from this checkout.
- `scripts/`: repeatable local build helpers.

## Rebuild

Build the default GTK frontend:

```sh
./scripts/rebuild.sh
```

Build another frontend:

```sh
./scripts/rebuild.sh framebuffer
./scripts/rebuild.sh monkey
```

Clean all local generated build output and dependency checkouts, then rebuild:

```sh
./scripts/clean-rebuild.sh gtk
```

Run all enabled unit tests:

```sh
./scripts/test.sh
```

Launch the GTK browser with JavaScript enabled:

```sh
./scripts/run-js.sh
./scripts/run-js.sh https://example.com
```

The scripts source `scripts/local-env.sh`, which sets:

```sh
TARGET_WORKSPACE=$PWD/projects
REPO_BASE_URI=https://github.com/netsurf-browser
TARGET_TOOLKIT=gtk3
```

Override those variables before running a script if needed.
