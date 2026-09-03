# Ubuntu 26.04 Debian Package

Slate's first Debian package is a testing package built natively on Ubuntu
26.04. It is not yet a Debian archive source package. The package is intended
to make installation and clean-machine testing repeatable while the browser is
still pre-alpha.

## Build

Prepare the pinned local Rust toolchain and confirm the native build
dependencies first:

```bash
make setup-local-rust
make check-tools
make deb
```

The package and its detached SHA-256 checksum are written under `dist/`:

```text
dist/slate-browser_<version>_amd64.deb
dist/slate-browser_<version>_amd64.deb.sha256
```

The generated version includes the current Git commit date and abbreviated
commit id. A `.dirty` suffix records that tracked or untracked workspace state
was present during the build. Set `SLATE_DEB_VERSION` to an explicit valid
Debian version when preparing a named release.

The first optimized Servo build can leave several gigabytes of disposable
compiler and staging data. Remove it after packaging without deleting `dist/`:

```bash
make deb-clean
```

The package build deliberately uses Ubuntu's shared SQLite, FreeType,
HarfBuzz, Fontconfig, zlib, and zstd libraries; the system C/C++ and graphics
libraries; and the shared Rust standard library. Servo remains embedded inside
the package-private `libslate_chrome.so`; it is not loaded as an independently
replaceable engine library.

## Install

Use APT so runtime dependencies are resolved:

```bash
(cd dist && sha256sum --check slate-browser_<version>_amd64.deb.sha256)
sudo apt install ./dist/slate-browser_<version>_amd64.deb
```

The installed layout is:

```text
/usr/bin/slate -> ../lib/slate/slate
/usr/lib/slate/slate
/usr/lib/slate/libslate_chrome.so
/usr/lib/slate/libstd-*.so
/usr/lib/slate/resources/resource_protocol/
/usr/share/applications/io.github.gabrieldiego.Slate.desktop
/usr/share/icons/hicolor/*/apps/slate.png
/usr/share/metainfo/io.github.gabrieldiego.Slate.metainfo.xml
```

Slate stores a newly created default profile at
`~/.slate/slate-settings.db`. An explicit `--settings-db` path takes priority,
and an existing `./slate-settings.db` continues to select portable-folder
behavior. Package installation and removal do not modify profile data.

The package contains no maintainer scripts and performs no network activity at
install or removal time.

## Verify

Inspect the package before installing it:

```bash
dpkg-deb --info dist/slate-browser_<version>_amd64.deb
dpkg-deb --contents dist/slate-browser_<version>_amd64.deb
```

Clean-machine testing should cover desktop-menu and terminal launch, Wayland
and X11 sessions, `slate://` resources, HTTPS navigation, local IPFS/IPNS
routing, profile persistence across reinstall, and absence of unexpected
startup network requests.
