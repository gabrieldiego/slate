# Broadweb Daemon

Status: draft

This note collects the current design direction for `broadwebd`, Slate's
managed local service for broadweb protocols.

See `docs/architecture/broadwebd-plugin-contract.md` for the concrete internal
plugin names, Rust interfaces, hot-loading boundary, and first IPFS service
contract.

`broadwebd` should make protocols such as IPFS, I2P, Tor, Gemini, Nostr,
Matrix, and future broadweb adapters usable without forcing every browser
window or mobile session to cold-start protocol state from nothing. It should
own protocol liveness, warm caches, background transfers, and service health.
It should not silently own browser policy.

## Goals

- Keep expensive protocol state warm when doing so is useful.
- Abstract broadweb transports into application-layer services that Slate can
  consume.
- Support a plugin-shaped architecture for protocol transports and future
  application services.
- Avoid turning Slate into a public server by default.
- Keep browser policy in `browser-core`, not in the daemon.
- Keep protocol integrations behind explicit adapter boundaries.
- Persist profile-scoped protocol state so restart behavior is testable.
- Support resource-constrained devices through strict budgets and degraded
  modes.
- Make daemon behavior testable without real IPFS, Tor, I2P, or public network
  access.
- Keep Slate-owned code in safe Rust.

## Non-Goals

- Do not run a full IPFS DHT node by default.
- Do not provide, reprovide, seed, pin, relay, or accept inbound connections
  without explicit user intent.
- Do not route private-network names through ordinary DNS.
- Do not fall back to public gateways unless browser policy explicitly permits
  that for the current profile and navigation.
- Do not place UI, renderer internals, or broadweb policy decisions inside the
  daemon.

## Process Model

The preferred long-term shape is:

```text
Slate UI / chrome
  -> browser-core
      -> protocols / routing policy
      -> broadwebd client

broadwebd
  -> lifecycle manager
  -> resource budget manager
  -> protocol service supervisors
  -> application-layer service adapters
  -> profile-scoped state store
  -> local IPC server
```

The browser process decides whether a navigation is allowed, which profile it
belongs to, whether private browsing changes the behavior, and what routing
mode may be used. The daemon executes approved protocol work within those
constraints.

The current implementation should remain in-process while the service API,
plugin registry, fetch boundary, and tests are still taking shape. This avoids
adding process supervision, IPC framing, authentication, and streaming failure
modes before the browser and daemon boundary is clear.

The later daemon process should be reachable only through a local IPC
mechanism, such as a Unix domain socket on Linux/macOS and a named pipe with
per-user access control on Windows. It should not expose a public network
control API.

## Transport And Application Layers

`broadwebd` should be treated as an abstraction layer between transport
protocols and application-layer services.

Transport protocols include IPFS, Tor, I2P, BitTorrent, delegated routing
services, local gateways, relays, proxies, and future distributed/private
networks. These protocols have very different connection models, bootstrap
costs, identity behavior, privacy boundaries, and persistence needs.

Application-layer services are the things Slate wants to consume after a route
has been approved. HTTP is the first and most important service because it lets
Slate display HTML pages through the existing browser and Servo rendering path.
Later services may include shared files, calendars, messaging, sync, publishing,
or other first-party Slate apps.

The first design target is therefore:

```text
Slate navigation
  -> browser-core policy
  -> broadwebd approved HTTP fetch
  -> transport-specific adapter
  -> HTTP-like response stream
  -> Servo renderer
```

This keeps Slate from having to know whether a page came through local IPFS,
I2P, Tor, a gateway, a proxy, or another broadweb route once browser-core has
approved the policy boundary. At the same time, it avoids pretending every
future service is only web page loading. HTTP should be the first application
service, not the only one the daemon can eventually expose.

Future non-HTTP services should get explicit APIs rather than being hidden
inside page fetch behavior. For example, file sharing, calendar sync, and
messaging should be modeled as separate application services with their own
permissions, storage, and privacy rules.

## Protocol And App Plugins

`broadwebd` should be designed around plugins, but the first implementation
should not require dynamic native code loading.

The initial plugin model should be a safe Rust registry of statically linked
adapters. This keeps the pre-alpha architecture simple, testable, and compatible
with Slate's safe Rust policy while still forcing protocol integrations through
clear plugin boundaries.

Three plugin categories should be kept distinct:

```text
protocol service plugins
  Own long-lived protocol configuration, state, health, and route planning.
  Examples: ipfs, tor, i2p, gemini, bittorrent.

transport plugins
  Own connectivity to a broadweb transport or routing environment.
  Examples: direct-http, ipfs-gateway, tor-socks, i2p-http-proxy.

application service plugins
  Expose an application-level capability on top of one or more transports.
  Examples: http-fetch, shared-files, calendar-sync, messaging, publishing.
```

Protocol services should own protocol configuration and choose the transport
adapter for an approved URL or service request. Transport plugins should expose
capabilities, health, startup cost, supported routes, and privacy boundaries.
They should not decide whether a navigation is allowed. Application service
plugins should receive an approved request and use one or more transport
plugins to produce an application-level result.

For the first milestone, `http-fetch` should be the only application service.
It should be able to use a `direct-http` transport for ordinary HTTP(S) and
later IPFS, I2P, Tor, or gateway transports for broadweb routes. This proves the
plugability of the path before Slate adds file sharing, calendars, messaging, or
other app surfaces.

The registry should be explicit:

```text
PluginRegistry
  -> protocol service plugins
      ipfs
      tor
      i2p
  -> transport plugins
      direct-http
      ipfs-gateway
      tor-socks
      i2p-http-proxy
  -> application service plugins
      http-fetch
      shared-files
      calendar-sync
```

The registry should support dependency declarations. For example, `http-fetch`
may depend on `direct-http` for normal web traffic, while a future
`shared-files` service may depend on IPFS or BitTorrent. Missing dependencies
must produce clear degraded health states rather than implicit fallbacks.

Dynamic or external plugins can be considered later, but they should use a
process boundary and IPC contract instead of loading untrusted native code into
the daemon. This keeps crashes, dependency conflicts, and unsafe implementation
details outside the core daemon process.

## Ownership Boundaries

`browser-core` owns:

- navigation policy;
- profile and private-window policy;
- permission decisions;
- public gateway fallback policy;
- whether a route may use a proxy, gateway, daemon, or direct network path.

`routing` owns:

- multiaddr parsing and validation;
- explicit route descriptions;
- route modes such as direct, gateway, proxy, internal, or daemon-backed.

`protocols` owns:

- protocol classification;
- adapter interfaces;
- conversion from user-facing addresses to route/fetch plans;
- protocol-specific validation rules.

`broadwebd` owns:

- protocol service lifecycle;
- plugin registration and capability discovery;
- warm peerstores, caches, and routing hints;
- background transfer workers;
- resource budgets;
- health and readiness reporting;
- durable protocol state under profile-specific roots.

`rendering` owns:

- the Servo embedding boundary;
- feeding approved protocol responses into Servo;
- blocking private-network hostnames before ordinary HTTP(S) routing when no
  safe adapter path exists.

## Capability Modes

`broadwebd` should support explicit modes rather than one always-on behavior.

```text
off
  No daemon work. Broadweb navigations either cold-start on demand or fail
  closed, depending on browser policy.

session
  Daemon stays warm while Slate is open. This is the likely desktop default and
  the highest mobile default that should be considered early.

background-light
  Keep small protocol state, health checks, active downloads, and paired-device
  control alive after the UI is closed. Full protocol workers may suspend when
  idle.

vpn-or-proxy
  OS-visible network mode for protocols that behave like a VPN or proxy. This
  should require clear user consent and visible status.

sync-or-provider
  Opt-in mode for pinning, publishing, seeding, reproviding, or other behavior
  that intentionally helps host or distribute content.

desktop-relay
  Mobile or low-memory clients delegate heavy broadweb work to a paired desktop
  or home server running Slate's daemon.
```

Closing the visible app should be allowed to put Slate into a background mode
when useful, but that must be visible and controllable. Expensive background
work should stop or degrade on battery saver, metered networks, low memory, or
thermal pressure.

## Resource Budgets

The daemon should have first-class budget settings from the beginning:

```text
max_idle_memory
max_cache_size_per_profile
max_peer_connections
max_protocol_workers
max_background_bandwidth
allow_metered_network
allow_background_on_battery
allow_inbound_connections
allow_reprovide
allow_public_gateway_fallback
```

Budgets should be profile-aware where practical. A private window should not
reuse persistent broadweb identity, routing state, or caches unless the user has
explicitly chosen that behavior.

Mobile should default to the most conservative useful behavior:

- session-bound protocol work;
- aggressive suspension after backgrounding;
- capped cache sizes;
- delegated routing where possible;
- no provider mode by default;
- no full DHT participation by default;
- no background work on metered networks or battery saver without consent.

Desktop can offer richer background modes, but provider/pinning/relay behavior
should still be explicit.

## State Layout

The daemon should keep durable state under a profile-scoped root:

```text
state-root/
  daemon.lock
  daemon.json
  profiles/
    default/
      profile.json
      protocol-state/
        ipfs/
          config.json
          peerstore/
          blockstore/
          routing-cache/
          pins/
        i2p/
          config.json
          proxy-state/
        tor/
          config.json
          control-state/
        nostr/
          config.json
          relay-cache/
        matrix/
          config.json
          sync-state/
      temporary/
  volatile/
```

The exact format can evolve, but the boundaries should remain clear:

- profile state must not leak between profiles;
- private browsing state should be ephemeral by default;
- volatile process state should be separated from durable protocol state;
- cache and blockstore size must be enforceable;
- state migrations must be testable.

## IPC Surface

The first implementation can be in-process for tests and early integration, but
the API should be designed so it can later move behind IPC without changing
browser policy code.

Slate-to-`broadwebd` control traffic should use OS-local IPC, not an exposed
TCP service. On Linux and macOS this should be a Unix domain socket under a
per-user runtime directory such as `$XDG_RUNTIME_DIR/slate/`. On Windows this
should be a named pipe with an ACL limited to the current user. The IPC
protocol should carry framed, request/response messages so long fetches,
progress events, cancellation, and streamed bodies do not require opening
additional public ports.

If Servo cannot route subresource loading through an embedding hook, Slate may
need a loopback HTTP proxy as a compatibility bridge for the renderer. That
proxy should be separate from the daemon control API, bound only to loopback,
randomized per session, token-protected, profile-scoped, and removable once a
direct Servo network hook exists. It should not become the primary
Slate-to-`broadwebd` API.

Candidate commands:

```text
Health()
ListProtocols()
ListPlugins()
ListApplicationServices()
StartProtocol(profile, protocol, mode)
StopProtocol(profile, protocol)
Plan(profile, navigation_target)
Fetch(profile, approved_fetch_plan)
CallService(profile, approved_service_request)
Cancel(request_id)
SubscribeEvents(profile)
SetBudget(profile, budget)
Shutdown()
```

Candidate events:

```text
ProtocolStarting
ProtocolReady
ProtocolDegraded
ProtocolStopped
PluginReady
PluginDegraded
FetchStarted
FetchProgress
FetchCompleted
FetchFailed
BudgetExceeded
BackgroundModeChanged
```

Policy-bearing inputs should be explicit. The daemon should receive an approved
plan from browser-core rather than independently deciding to use a public
gateway, proxy, or private network.

The IPC boundary should preserve the same ownership split as the in-process
API: browser-core approves policy and profile context, while `broadwebd`
executes approved work through registered transport and application-service
plugins. Fetch responses should carry route context back across the boundary:
the approved profile id, selected transport id, and selected transport privacy
boundary. Slate can then surface local-node, public-gateway, proxy, or direct
network behavior without guessing from URLs.

## IPFS Initial Shape

IPFS should be the first broadweb protocol to use the daemon because `ipfs://`
and `ipns://` are direct schemes and already fit Slate's protocol callback path.

Initial IPFS mode should be retrieval-focused:

- support local gateway or local RPC when configured;
- support explicitly configured public gateway retrieval when browser policy
  permits it;
- support verified/trustless retrieval where practical;
- use delegated routing before full local DHT participation;
- persist cache and routing hints;
- avoid advertising viewed CIDs by default;
- avoid inbound connections by default;
- keep public gateway fallback disabled by default.

Full DHT participation, pinning, providing, and publishing should be later
capabilities with explicit resource and privacy controls.

The current default session uses a local gateway at `http://127.0.0.1:8080`.
Manual runs can override it with `SLATE_IPFS_GATEWAY`. Public gateway testing
also requires `SLATE_IPFS_GATEWAY_SCOPE=public`; otherwise non-loopback
gateways are rejected instead of becoming silent public fallback.

The daemon also has an opt-in `ipfs-kubo-rpc` transport for local Kubo nodes.
It maps `ipfs://` and `ipns://` fetches to the loopback `/api/v0/cat` RPC and
infers common web content types for HTML, CSS, JavaScript, and image assets.
Directory-style paths retry `<path>/index.html` after a failed `cat` response.
Manual runs can select it with `SLATE_IPFS_TRANSPORT=kubo-rpc` and optionally
override the loopback endpoint with `SLATE_IPFS_KUBO_RPC`. This is an
integration point for local-node retrieval, not a public RPC mode and not the
default browsing path.

Both gateway and Kubo retrieval share broadwebd's render-vs-download content
classification. Specific `Content-Type` headers win, but generic binary
responses can still be treated as HTML when the body or path clearly identifies
an HTML document. This keeps simple IPFS/IPNS websites renderable when a gateway
or local node returns weak MIME metadata.

When classification marks a response as a download, broadwebd writes the body to
`profiles/<profile>/temporary/downloads/` and attaches a `DownloadRecord` to the
response. The current renderer shows a download-ready page with the saved path
and size; the future downloads UI should own promotion to user-selected
persistent storage, removal, verification, and progress/history presentation.

## Mobile And Paired Devices

Mobile broadweb support should be designed around constrained resources. It may
work more like a VPN or file-sync app than like a traditional always-running
desktop daemon.

Mobile defaults should favor:

- foreground or session-bound work;
- short grace periods after backgrounding;
- OS-visible foreground service or VPN modes when continuous work is active;
- Wi-Fi and charging requirements for heavy sync/provider work;
- pairing with a desktop or home Slate daemon for expensive protocol tasks.

In paired mode, the mobile device can act as a controller and display surface
while the desktop daemon owns the heavy protocol state. This may later extend to
remote rendering, but renderer hosting should be treated as a separate design
from the first `broadwebd` milestone.

## Testing Strategy

Tests should verify durable daemon behavior without depending on test order or
real user profile state.

### Ephemeral Per-Test State

Each unit or integration test should create a temporary state root:

```text
tmp-test-root/
  profiles/
    test-profile/
      protocol-state/
```

The test owns the root, starts the daemon service against it, and deletes it
after completion.

### Fixture State

For restart and migration tests, fixtures should be copied into a temporary
directory before each test. The test may mutate the copy and compare resulting
state to expected files.

Useful fixtures:

- empty profile;
- profile with warm IPFS routing cache;
- profile with cache over budget;
- private-window ephemeral profile;
- corrupted state file;
- old-version state requiring migration.

### Restart Tests

State continuity should be tested inside one test:

```text
create temp state root
start daemon
perform protocol operation
shutdown daemon
start daemon again with same state root
assert expected state was reused or rejected
```

This proves persistence without creating ordering dependencies between tests.

### Fake Protocol Adapters

The first daemon tests should use fake adapters before real IPFS/Tor/I2P:

- fake adapter with deterministic success;
- fake adapter that delays startup;
- fake adapter that exceeds memory/cache budget;
- fake adapter that requires background permission;
- fake adapter that attempts forbidden fallback.

Real protocol tests should use local fixtures or mock daemons. Tests must not
require live IPFS, Tor, I2P, public gateways, or external network availability
unless they are explicitly marked as external/manual.

Loopback network tests and external internet tests should be separate. Loopback
tests may bind `127.0.0.1` to exercise the daemon's HTTP path without relying on
public network availability. External tests may contact stable public endpoints,
but they must be ignored by default and gated behind an explicit environment
variable such as `SLATE_EXTERNAL_NETWORK_TESTS=1`.

## Implementation Slices

Recommended order:

1. Add `crates/broadwebd` as an in-process safe Rust service crate.
2. Add state-root types and profile-scoped storage helpers.
3. Add lifecycle states: stopped, starting, ready, degraded, stopping.
4. Add resource budget structures and budget enforcement tests.
5. Add fake protocol adapter tests.
6. Add static plugin registry types for transport and application services.
7. Add an IPC-neutral client/service trait.
8. Add `direct-http` transport and `http-fetch` application service.
9. Add IPFS gateway/delegated-retrieval adapter.
10. Connect browser-core route policy to the daemon client.
11. Add a real daemon binary only after the in-process service is testable.
12. Add platform service integration for background behavior.

## Open Questions

- What should the daemon binary be named: `broadwebd`, `slated`, or
  `slate-broadwebd`?
- Which Rust IPC crate or small protocol should implement framed local IPC
  without introducing unsafe Slate-owned code?
- What minimal loopback proxy shape is needed if Servo subresource loading
  cannot be routed through a direct embedding hook?
- What metadata should every plugin expose for capability, privacy, resource,
  and dependency discovery?
- Should external plugins ever be supported, or should plugins remain built-in
  until the broadweb security model is mature?
- Should IPFS local Kubo integration come before Slate-owned verified retrieval?
- What resource budgets should be default on desktop?
- What reduced defaults should mobile use?
- How should private windows interact with warm daemon state?
- How should paired desktop/mobile mode authenticate devices and revoke access?
- Which daemon state formats need stable migrations before pre-alpha release?
