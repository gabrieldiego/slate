# Slate

Slate is an early-stage privacy-focused broadweb browser project. It aims to provide one coherent browser experience for the conventional web and for distributed, private, peer-to-peer, local-first, and alternative protocol spaces.

Slate uses Servo as its primary rendering engine and is designed around safe Rust for Slate-owned code.

## Status

Slate is currently a scaffolded Rust workspace with an initial runnable browser-shell mockup. The first UI is code-native and portable: Slate draws its own browser chrome into a framebuffer and uses a small windowing layer to show it.

The current renderer is a `ServoBackend` boundary in `crates/rendering/`. Real Servo embedding is the next step behind that boundary.

## Goals

- Build Slate-owned code entirely in safe Rust.
- Use Servo as the main rendering engine.
- Vendor Servo from `https://github.com/gabrieldiego/servo` under `third_party/servo`.
- Keep Slate-to-Servo integration behind a narrow rendering boundary.
- Make broadweb browsing understandable and explicit for users.
- Support conventional HTTPS first, then add protocols such as IPFS/IPNS, Tor `.onion`, I2P, and future adapters incrementally.
- Use multiaddr internally for routing targets, protocol service endpoints, proxies, gateways, and peers where practical.
- Avoid telemetry and unexpected network activity by default.

## Planned Layout

```text
crates/
  slate/
  apps/
  browser-core/
  chrome/
  net/
  platform/
  privacy/
  protocols/
  rendering/
  routing/
  storage/
docs/
  adr/
  architecture/
  privacy/
  protocols/
third_party/
  servo/
tests/
  fixtures/
  integration/
xtask/
```

## Running

Install the platform libraries needed by `minifb` on your OS, then run:

```bash
cargo run -p slate
```

or:

```bash
scripts/run.sh
```

Check the workspace with:

```bash
cargo check --workspace
```

or:

```bash
scripts/check.sh
```

The window opens a Slate mockup with the intended first shape: left app rail, tab strip, navigation toolbar, address bar, and a quiet home viewport. The side rail switches between Web, Downloads, Calendar, and Messaging panes; tabs can be selected; and the `+` tab button creates a mock tab.

## Servo

Servo should be vendored as a pinned git submodule:

```bash
git submodule add git@github.com:gabrieldiego/servo.git third_party/servo
git -C third_party/servo remote add upstream https://github.com/servo/servo.git
git -C third_party/servo checkout <pinned-tag-or-commit>
```

Slate crates should depend on Servo through the vendored crate path:

```toml
servo = { path = "third_party/servo/components/servo" }
```

Some Servo patches are expected to be necessary. Keep them small, documented, easy to rebase, and suitable for upstream submission when they are not Slate-specific.

GitHub deploy keys are repository-scoped. Use a dedicated key for the Servo fork, not the Slate repository key.

## License

Slate-owned code is intended to be licensed under the GNU General Public License, version 3 or later:

```text
SPDX-License-Identifier: GPL-3.0-or-later
```

See [LICENSE](LICENSE) for the GPLv3 text.

Vendored third-party code keeps its own license. In particular, Servo is MPL-2.0 licensed and should remain under `third_party/servo` with its original license notices preserved.
