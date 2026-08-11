# AGENTS.md

Guidance for AI agents working on Slate, a privacy-focused broadweb browser that uses Servo as its primary rendering engine and integrates distributed and private browsing protocols such as IPFS, Tor, I2P, and related systems.

## Product Direction

Slate is a browser, not a browser-themed demo. Build toward a usable desktop browser with clear separations between rendering, browser chrome, networking, protocol adapters, storage, profile state, and privacy controls.

Core goals:

- Write Slate-owned code entirely in safe Rust.
- Use Servo as the main web rendering engine.
- Make the broadweb accessible through a coherent browser experience.
- Support conventional HTTPS browsing first, then add protocol adapters incrementally.
- Integrate distributed/private protocols through explicit adapter boundaries.
- Treat privacy, security, and user control as product requirements, not optional polish.
- Keep the architecture testable enough that protocol handling and privacy behavior can be verified without launching the full browser.

Non-goals for early work:

- Do not create additional Servo forks or long-lived divergent Servo patches casually.
- Do not add `unsafe` blocks, `unsafe` functions, or unsafe trait implementations to Slate-owned code.
- Do not implement cryptographic, anonymity, or peer-to-peer protocols from scratch when mature implementations exist.
- Do not mix browser UI state, network routing, and renderer internals in the same module.
- Do not add telemetry, analytics, or remote logging unless explicitly requested and reviewed.

## Naming And Terminology

Use `chrome` for Slate's browser UI layer intentionally. This is the historical browser term for the interface surrounding rendered content, including tabs, toolbars, side panes, menus, settings, downloads, and permission prompts. It does not refer to Google Chrome.

The name Slate can be understood as a nod to a very early surface for expression: a simple frame or medium that lets marks, symbols, and messages become visible. Treat the name as aligned with the browser's purpose: a quiet, durable surface for broadweb navigation rather than a decorative brand concept.

## Broadweb Purpose

Slate's main purpose is broadweb browsing: giving users one coherent browser for the conventional web and for distributed, private, peer-to-peer, local-first, and alternative protocol spaces.

In Slate, broadweb means:

- The normal web, including `https://` and `http://`.
- Content-addressed and distributed protocols such as `ipfs://` and `ipns://`.
- Privacy-preserving networks and names such as `.onion` and I2P destinations.
- Other user-chosen protocols that can be integrated through safe, explicit adapters.

Broadweb does not mean silently routing everything everywhere. Every non-standard protocol must have clear routing, privacy, permission, and failure behavior. Users should understand which network they are using and what trust boundary applies.

Use multiaddr as Slate's internal representation for broadweb routing targets and protocol service endpoints. User-facing navigation may use URLs such as `https://`, `ipfs://`, `ipns://`, and `slate://`, but internal routing decisions should resolve to explicit multiaddrs where practical.

Examples:

```text
/ip4/127.0.0.1/tcp/8080/http
/ip4/127.0.0.1/tcp/5001/http
/ip4/127.0.0.1/tcp/9050/socks5
/ip4/127.0.0.1/tcp/4444/http
/dnsaddr/bootstrap.libp2p.io/p2p/<peer-id>
```

Use multiaddr to describe how Slate reaches a service or peer; do not confuse it with content identity. CIDs identify IPFS content, IPFS/IPNS URLs identify navigable resources, and multiaddrs identify routing paths, transports, peers, proxies, gateways, and local protocol services.

## Safe Rust Policy

Slate-owned code must be written entirely in safe Rust.

- Add `#![forbid(unsafe_code)]` to every Slate-owned Rust crate.
- Do not use `unsafe`, including isolated blocks, unsafe functions, unsafe trait implementations, FFI wrappers, or unchecked conversions.
- Prefer safe abstractions from the Rust standard library and well-maintained crates.
- If operating system, Servo, graphics, networking, or sandboxing integration appears to require unsafe code, isolate the requirement behind an external dependency or upstream API instead of adding unsafe code to Slate.
- Treat dependencies that contain unsafe code as security-sensitive. Review why they are needed, whether a safe alternative exists, and what boundary contains them.
- Do not weaken safe Rust guarantees for performance without an explicit architecture decision record and maintainer approval.

## Preferred Architecture

Use a layered design:

1. `slate`: Desktop application binary and composition root. Wire crates together here; avoid business logic in the binary.
2. `chrome`: Browser UI shell, tabs, address bar, side app pane, settings, history views, downloads UI, menus, and permission prompts.
3. `apps`: First-party side-pane app registry and app surfaces such as web, downloads, calendar, and messaging.
4. `browser-core`: Navigation model, session state, profiles, permissions, tab lifecycle, and coordination between components.
5. `rendering`: Servo embedding, webview lifecycle, compositor integration, input forwarding, and page events.
6. `net`: HTTP(S), request policy, proxy handling, DNS policy, cache policy, fetch execution, and network isolation.
7. `routing`: Multiaddr parsing, routing plans, endpoint policy, proxy/gateway selection, and DNS-leak prevention.
8. `protocols`: Broadweb adapter layer for non-standard schemes such as `ipfs://`, `ipns://`, `.onion`, `i2p`, `magnet:`, `gemini://`, or future protocols.
9. `privacy`: Site isolation policy, fingerprinting defenses, storage partitioning, private windows, identity containers, and leak prevention.
10. `storage`: Profiles, bookmarks, history, cookies, caches, downloads, and encrypted local state where appropriate.
11. `platform`: OS integration, sandboxing, keychain/secret storage, file pickers, notifications, and updater hooks.
12. `xtask`: Developer automation for repeatable build, test, packaging, vendoring, and release tasks.

Keep dependencies flowing inward: `slate` wires the application together; `chrome` may call `browser-core` and `apps`; `browser-core` may coordinate `rendering`, `net`, `routing`, `protocols`, `privacy`, and `storage`; low-level adapters should not import UI code.

## Servo Integration

Prefer embedding Servo through its supported embedding APIs and upstream patterns. Slate uses vendored Servo because browser-level integration will likely require patches over time. Patches are acceptable when the embedding API, protocol integration, privacy model, UI integration, sandboxing, or platform behavior cannot be implemented cleanly from Slate-owned crates alone.

When a missing feature requires Servo changes:

- First isolate the need behind a Slate-owned trait or interface.
- Document the exact Servo limitation and the expected upstream API.
- Keep local patches small, purposeful, and easy to rebase.
- Prefer upstream contributions over long-lived private patches. If a Servo patch is not Slate-specific, prepare it for upstream submission.

Do not assume Chromium or Gecko APIs exist. Design around Servo's actual embedding surface and add compatibility shims only where they reduce Slate-specific complexity.

Slate expects Servo to be vendored through the project fork at `https://github.com/gabrieldiego/servo`, not copied directly into Slate-owned crates. Prefer this layout:

```text
third_party/
  servo/
```

Initial checkout should use a pinned git submodule:

```bash
git submodule add https://github.com/gabrieldiego/servo.git third_party/servo
git -C third_party/servo remote add upstream https://github.com/servo/servo.git
git -C third_party/servo checkout <pinned-tag-or-commit>
```

Slate crates should depend on Servo through its crate path, for example from the workspace root:

```toml
servo = { path = "third_party/servo/components/servo" }
```

Servo vendoring and patch rules:

- Keep Servo code under `third_party/servo`; do not mix Servo source files into `crates/`.
- Record the pinned Servo commit in the Slate lockfile, release notes, or an architecture note.
- Keep Slate-specific Servo patches on named branches in `gabrieldiego/servo`.
- Expect some Servo patches to be inevitable; the goal is controlled divergence, not zero patches.
- Rebase the fork against upstream Servo regularly, especially before large Slate rendering changes.
- When a patch could benefit Servo embedders generally, prepare it so it can be proposed upstream.
- Before keeping a Servo patch private, document why it should not be submitted upstream yet.
- For upstreamable patches, keep commit history, tests, and descriptions suitable for a Servo pull request.
- Document each Slate-specific Servo patch with the reason it exists, the Slate feature it supports, and what would allow it to be removed.
- Treat Servo and Servo patches as third-party engine code. The Slate safe Rust policy applies to Slate-owned crates, not to Servo's internal implementation.
- Do not add `#![forbid(unsafe_code)]` to Servo crates unless that change is accepted by Servo upstream.
- Keep the Slate-to-Servo boundary narrow, preferably inside `crates/rendering/`.

## Protocol Adapter Rules

All distributed/private protocol support must go through a common adapter interface. Each adapter should define:

- Supported schemes and address forms.
- Multiaddr routing target format for local services, gateways, proxies, peers, or daemons.
- How names are resolved.
- How requests are fetched or streamed.
- Privacy risks and leak boundaries.
- Cache behavior.
- Permission prompts, if any.
- Failure modes and user-facing errors.
- Test fixtures that do not require live external networks.

Protocol-specific guidance:

- IPFS/IPNS: prefer using a local node or well-defined gateway configuration. Never silently fall back to a public gateway without user-visible policy.
- Tor: route `.onion` and Tor-mode traffic through Tor explicitly. Prevent DNS leaks and mixed identity routing.
- I2P: keep I2P routing separate from normal HTTP routing. Make proxy and router assumptions explicit.
- Local gateways: treat local gateway ports as privileged configuration, not hard-coded constants.
- Unknown schemes: fail closed with a clear error unless a registered adapter exists.

Routing guidance:

- Convert user-facing navigation targets into an explicit routing plan before fetching.
- Represent local protocol services, proxies, gateways, and peer endpoints with multiaddr where practical.
- Keep multiaddr parsing and validation centralized; do not hand-parse multiaddr strings in individual adapters.
- Reject malformed or unsupported multiaddrs before any network activity.
- Do not allow private-network protocol names to fall through to normal DNS or direct HTTP routing.

## Privacy And Security Requirements

Default to conservative behavior:

- No unexpected network requests during startup.
- No remote telemetry by default.
- No public gateway fallback without consent.
- No DNS resolution for names that must be handled by private networks.
- No cross-profile cookie, cache, history, or identity sharing.
- No persistent permission grant without a visible settings surface.
- No plaintext storage of secrets, tokens, or private keys.

For every feature that touches network, storage, identity, or permissions, document:

- What data leaves the machine.
- Which process or service receives it.
- Whether it is profile-specific.
- Whether private browsing changes the behavior.
- How the behavior is tested.

## Development Practices

Before editing:

- Inspect the existing tree and follow local patterns.
- Check for an existing `AGENTS.md` in the current or parent directory.
- Read relevant README, build, and test files before introducing new tools.

While editing:

- Keep changes narrowly scoped.
- Prefer small interfaces with explicit ownership over global state.
- Add comments only when they explain non-obvious security, privacy, or protocol behavior.
- Avoid committing generated files, caches, local node state, downloaded protocol data, or secrets.
- Keep configuration example files safe by default.

When adding dependencies:

- Prefer maintained libraries with active security posture.
- Avoid dependencies that perform implicit telemetry or background networking.
- Record why the dependency is needed.
- Keep protocol implementations behind interfaces so they can be replaced.

## Testing Expectations

Add tests at the level where behavior can be verified with the least machinery:

- Unit tests for URL parsing, scheme dispatch, permission decisions, and routing policy.
- Integration tests for protocol adapters using local fixtures or mock daemons.
- Browser-core tests for navigation, tab lifecycle, private windows, profile isolation, and permission persistence.
- UI tests for workflows that users depend on: navigation, error pages, settings, downloads, and protocol prompts.
- Leak tests for DNS, proxy bypass, profile boundaries, and private browsing persistence.

Tests must not depend on live IPFS, Tor, I2P, or public network availability unless explicitly marked as external/manual.

## Repository Shape

Use this as the baseline layout unless a documented architecture decision changes it:

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

Layout guidance:

- `crates/slate/` is the desktop app entrypoint and composition root.
- `crates/chrome/` owns browser UI shell behavior, not rendered web content.
- `crates/apps/` owns first-party side-pane apps and their registration.
- `crates/rendering/` is the normal Slate-owned boundary into Servo.
- `crates/routing/` owns multiaddr-centered routing plans and endpoint validation.
- `third_party/servo/` contains the vendored Servo fork and must remain outside Slate-owned crates.
- `xtask/` contains safe Rust developer automation instead of ad hoc scripts when practical.

Adjust this layout only when the chosen UI toolkit, Servo embedding approach, or packaging strategy makes another structure clearly better, and record the reason in `docs/adr/`.

## Documentation Expectations

Keep documentation close to decisions:

- `docs/adr/` for major decisions such as UI toolkit, process sandboxing, updater design, vendored Servo policy, and protocol daemon strategy.
- `docs/architecture/` for component boundaries and process model.
- `docs/protocols/` for scheme-specific behavior and threat notes.
- `docs/privacy/` for privacy guarantees, non-guarantees, and test plans.

Every new protocol adapter should include a short design note before or alongside implementation.

## Agent Handoff Checklist

Before ending a substantial task, report:

- What changed.
- What files were touched.
- What tests were run.
- What privacy/security assumptions were made.
- Any follow-up work that blocks production readiness.

If you cannot verify a claim locally, say so directly.
