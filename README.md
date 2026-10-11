# Slate

Slate is an early-stage privacy-focused broadweb browser project. It aims to provide one coherent browser experience for the conventional web and for distributed, private, peer-to-peer, local-first, and alternative protocol spaces.

Slate uses Servo as its primary rendering engine. Safe Rust is the policy for
Slate-owned code; the inherited browser shell still needs the
[chrome safe Rust migration](docs/roadmap.md#chrome-safe-rust-migration).

## Status

Slate is an experimental Rust workspace with a desktop browser shell derived
from Servo's shell. The current UI uses `egui`, `winit`, and `egui_glow`/OpenGL.
The shell embeds Servo directly for webviews, page rendering, and input. See
[UI architecture](docs/architecture/ui.md) for the current component boundaries
and headless chrome verification.

`crates/rendering/` also exposes a `ServoBackend` that captures software-rendered
page bitmaps for browser-core and rendering tests. It is separate from the
desktop shell's interactive embedding. [Servo integration](docs/architecture/servo.md)
describes the different navigation paths and the remaining boundary work.

Profile sync has signed, encrypted objects and replicated app metadata with
local fixture coverage. The Files page is still a mockup; complete file-content
synchronization and live Iroh/libp2p sync adapters remain unfinished. See
[profile sync](docs/architecture/profile-sync.md) for current scope.

## Goals

- Build Slate-owned code entirely in safe Rust.
- Use Servo as the main rendering engine.
- Vendor Servo from `https://github.com/gabrieldiego/servo` under `third_party/servo`.
- Keep Slate-to-Servo integration behind a narrow rendering boundary.
- Make broadweb browsing understandable and explicit for users.
- Support conventional HTTPS first, then add protocols such as IPFS/IPNS, Tor `.onion`, I2P, and future adapters incrementally.
- Use multiaddr internally for routing targets, protocol service endpoints, proxies, gateways, and peers where practical.
- Avoid telemetry and unexpected network activity by default.

## Workspace Layout

```text
crates/
  slate/
  apps/
  browser-core/
  broadwebd/
  chrome/
  net/
  platform/
  profile-sync/
  privacy/
  protocols/
  rendering/
  routing/
  storage/
docs/
  adr/
  architecture/
  protocols/
third_party/
  servo/
vendor/
  webrender-0.70.0/
xtask/
```

## Running

Use Rust 1.97.1, as pinned in `rust-toolchain.toml`, plus the platform libraries
needed by Servo and the `winit`/OpenGL desktop shell on your OS. Initialize the
[pinned Servo submodule](#servo) before invoking Cargo, including focused tests;
Cargo must load its manifests when resolving this workspace.
For a repository-local toolchain and Cargo cache, run:

```bash
make setup-local-rust
make check-tools
```

The Makefile build and test targets use `./.rustup` and `./.cargo` by default
so large Rust toolchains and crate caches stay with the checkout instead of
filling a small home or root filesystem.

Servo's Rust and native dependency graph is memory-heavy. Slate's checked-in
defaults keep build pressure conservative: Cargo builds use one job, test
harnesses use one thread, and Makefile/xtask Cargo invocations run through
`scripts/with-build-limits.sh` with a 6144 MiB address-space cap. Override those
knobs only on machines with enough RAM:

```bash
CARGO_BUILD_JOBS=2 SLATE_TEST_THREADS=2 SLATE_BUILD_MEMORY_LIMIT_MB=8192 make test-network
```

Set `SLATE_BUILD_MEMORY_LIMIT_MB=0` to disable the wrapper limit for a single
command.

Then run through the Makefile:

```bash
make run
```

Direct Cargo commands still work when your shell already has the pinned Rust
toolchain active:

```bash
cargo run -p slate
```

Check the workspace with:

```bash
make check
```

or:

```bash
scripts/check.sh
```

Broadwebd has focused test targets:

```bash
make test-broadwebd
make test-network
make test-external-network
```

Profile-sync boundary checks and deterministic chrome verification have their
own targets:

```bash
make profile-sync-boundary-static-check
make profile-sync-boundary-check
make chrome-verify
```

The static boundary target does not compile Rust. The full boundary target runs
focused sync tests; `chrome-verify` renders the chrome fixtures and writes
images and a report under `target/slate-chrome-verification/`.

`test-broadwebd` runs the daemon and plugin unit tests. `test-network` runs those
tests and invokes a browser-core filter named `navigating_http_hands_page_to_servo`.
That test name is no longer present: the current
`navigating_http_hands_address_to_surface_loader` test uses an injected loader.
Until the target is repaired, `test-network` does not establish additional
browser-core or loopback integration coverage.
`test-external-network` is opt-in and enables ignored internet smoke tests,
including public IPFS gateway retrieval, with `SLATE_EXTERNAL_NETWORK_TESTS=1`.

### IPFS Browsing

By default, IPFS/IPNS navigation tries a local gateway at
`http://127.0.0.1:8080` first. If that local gateway is unavailable or cannot
return a `200` response, broadwebd walks a hardcoded public gateway list and
caches the first working gateway for later IPFS/IPNS requests. The cache is
rotated on failure and reset to the original gateway after one bounded pass
through the list. Gateways that return the known IPFS service-worker bootstrap
page are treated as failed candidates because Slate needs direct HTTP responses
from broadwebd at this stage.

This fallback currently happens without a consent prompt, including with the
default local gateway. A public gateway receives requested CIDs/IPNS names and
request/network metadata. The
[consent policy](AGENTS.md#privacy-and-security-requirements) remains the product
target; enforcing it is deferred implementation work. The configuration below
describes current behavior, not an enforced consent gate.

To point Slate at a different first-choice gateway, launch it with
`SLATE_IPFS_GATEWAY=http://127.0.0.1:<port>`. To make the first choice a public
gateway, launch with `SLATE_IPFS_GATEWAY=https://ipfs.filebase.io` and
`SLATE_IPFS_GATEWAY_SCOPE=public`; if public scope is set without a gateway,
Slate uses its default public gateway list. These environment variables are the
temporary manual override path for IPFS browsing configuration. To
use a local Kubo RPC endpoint instead of a gateway, launch with
`SLATE_IPFS_TRANSPORT=kubo-rpc`; the default endpoint is
`http://127.0.0.1:5001` and can be overridden with
`SLATE_IPFS_KUBO_RPC=http://127.0.0.1:<port>`. Kubo RPC mode has no public gateway
fallback and cannot be combined with the gateway/scope overrides. The address
bar accepts canonical IPFS/IPNS URLs, common path-style forms such as `/ipfs/<cid>/...` and
`ipns/<name>/...`, and bare CIDv0/CIDv1 values. Slate normalizes those to
`ipfs://` or `ipns://` before fetching them through broadwebd.

The window opens a Slate shell with the intended first shape: left app rail,
navigation toolbar, address bar, Web tab previews under the rail, and a quiet
home viewport. The rail includes Home, Web, Downloads, Calendar, Chat, Contacts,
and Files. Rail apps are internal HTML pages under `slate://`:
`slate://home` is the static personal start page, `slate://web` is a singleton
broadweb front page currently backed by local browsing history,
`slate://downloads` exposes download state and controls, and `slate://calendar`
is the initial calendar mock. `slate://chat` is the initial chat aggregator mock for
SMS, WhatsApp, and future provider adapters. The `+` Web row opens
`slate://blank` with the address bar ready for input. The address bar can
navigate to Servo-rendered internal pages such as `slate://home`, local
HTML files such as `examples/local-page.html`, HTTP(S) pages, IPFS/IPNS routes,
and the initial Tor HTTP adapter. Protocol availability differs between the
desktop shell and the software-rendered adapter; see
[navigation paths](docs/architecture/servo.md#navigation-paths).

## Servo

Servo is already registered as a pinned git submodule. From an existing Slate
checkout, initialize the recorded revision:

```bash
git submodule update --init --recursive
```

For anonymous HTTPS access, set the local URL before running that command:

```bash
git config submodule.third_party/servo.url https://github.com/gabrieldiego/servo.git
```

If the remote cannot supply the recorded revision, obtain that exact commit
from the maintainer's clone or fork. Substituting the latest Servo revision
changes the dependency being validated.

Slate crates depend on Servo through the vendored crate path:

```toml
servo = { path = "third_party/servo/components/servo" }
```

The desktop shell and the software-rendered `ServoBackend` both delegate HTML,
CSS, and JavaScript to Servo and its SpiderMonkey integration. Slate does not
implement its own web-content renderer. Their embedding and fetch boundaries
are described in [Servo integration](docs/architecture/servo.md).

Broadwebd is currently an in-process service with a static plugin registry,
HTTP fetch, IPFS gateway/Kubo retrieval, and an initial embedded Arti Tor
transport. The desktop shell registers broadwebd handlers for `ipfs://`,
`ipns://`, `tor+http://`, and `tor+https://`; ordinary HTTP(S) navigation uses
Servo's network stack. Public IPFS gateway fallback follows the behavior
described above. `.onion` address-bar input is normalized to the Tor schemes;
Tor HTTPS is still rejected pending TLS support over Arti. See the
[Tor adapter limitations](docs/protocols/tor.md). I2P, Gemini, and Magnet support
is not implemented; placeholder rendering in `ServoBackend` does not provide
those protocols in the desktop shell.

GitHub deploy keys are repository-scoped. Use a dedicated key for the Servo fork, not the Slate repository key.

## License

Slate-owned code is intended to be licensed under the GNU General Public License, version 3 or later:

```text
SPDX-License-Identifier: GPL-3.0-or-later
```

See [LICENSE](LICENSE) for the GPLv3 text.

Vendored third-party code keeps its own license. In particular, Servo is MPL-2.0 licensed and should remain under `third_party/servo` with its original license notices preserved.
