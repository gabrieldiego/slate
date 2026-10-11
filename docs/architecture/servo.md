# Servo Integration

Servo is vendored at `third_party/servo` from the project fork:

```text
git@github.com:gabrieldiego/servo.git
```

The submodule should also keep upstream Servo configured as:

```text
https://github.com/servo/servo.git
```

Initialize the revision recorded by Slate with `git submodule update --init
--recursive`. See the [checkout instructions](../../README.md#servo) for the
local HTTPS override. A missing pinned commit must be recovered before building;
checking out another Servo revision changes the dependency under test.

## Current Embedding Paths

Slate currently has two Servo embeddings:

- The desktop application enters `slate_chrome::main`. The
  [desktop app](../../crates/chrome/src/desktop/app.rs) constructs Servo,
  registers custom protocols, and manages interactive webviews through the
  inherited shell. `winit` supplies window events, while the shell forwards
  page input and composes Servo content with the egui chrome.
- [ServoBackend](../../crates/rendering/src/lib.rs) implements `RenderBackend`
  for browser-core and rendering tests. It creates a software rendering context,
  loads a page in a Servo `WebView`, and captures a bitmap. It is not the desktop
  shell's interactive rendering path.

Both embeddings delegate HTML, CSS, and JavaScript to Servo and SpiderMonkey.
The architectural goal remains a narrow Slate-owned rendering boundary. The
desktop shell's direct Servo imports are current integration debt, not evidence
that all callers already use `crates/rendering/`. Native shell safety work is
tracked by the [chrome migration TODO](../roadmap.md#chrome-safe-rust-migration).

## Navigation Paths

| Navigation | Desktop shell | Software-rendered `ServoBackend` |
| --- | --- | --- |
| Ordinary HTTP(S) | Servo's normal network stack | Top-level fetch through broadwebd, then HTML passed to Servo |
| `ipfs://`, `ipns://` | Registered broadwebd protocol handler for pages and subresources | Top-level broadwebd fetch and registered handler for subresources |
| `tor+http://`, `tor+https://` | Registered broadwebd Tor handler | Top-level broadwebd fetch and registered handler for subresources |
| `i2p://`, `gemini://`, `magnet:` | No corresponding custom handler registered | Local placeholder pages only |

The desktop [address parser](../../crates/chrome/src/parser.rs) normalizes
`.onion` input to `tor+http://` or `tor+https://`. The Arti transport implements
Tor HTTP; Tor HTTPS is rejected until TLS over Arti exists. Absolute private-name
subresources still need the integration work described in the
[Tor adapter](../protocols/tor.md). `ServoBackend` blocks direct private-name
HTTP(S) addresses instead of sending them to normal web routing. Neither
placeholder pages nor address-bar normalization establish complete private
network coverage.

The desktop [broadweb handler](../../crates/chrome/src/desktop/protocols/broadweb.rs)
passes navigation and subresource requests to the in-process daemon. Navigation
errors and downloads receive local HTML response pages; subresources use the
subresource request purpose without creating user download records. Explicit
desktop download requests also use broadwebd. This does not route every ordinary
HTTP(S) desktop page request through the daemon.

The software adapter adds a document `<base>` to fetched broadweb HTML when one
is absent, using the effective URL so relative assets and Kubo `index.html`
fallbacks resolve correctly. It exposes broadwebd route context in render
metrics. IPFS browsing currently uses local-first gateway retrieval with public
fallback; the [current behavior and consent gap](../../README.md#ipfs-browsing)
apply to the daemon's gateway path. Kubo RPC mode has no public gateway fallback.

## Verification Scope

The rendering crate groups Servo-backed page tests into one headless smoke test
because Servo process-global options can only be initialized once per test
process. IPFS/IPNS cases use an injected local fixture daemon, including a
relative stylesheet case; these tests do not require public gateways.

`make chrome-verify` exercises the egui chrome drawing path and emits images and
crop checks. Chrome screenshots alone do not validate web-content rendering,
protocol routing, or the desktop event loop. See [UI architecture](ui.md).

GitHub deploy keys are repository-scoped. Use the Slate deploy key for `gabrieldiego/slate` and a separate Servo deploy key for `gabrieldiego/servo`.
