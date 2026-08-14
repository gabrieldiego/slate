# Broadwebd Plugin Contract

Status: draft

This note defines the first internal contract for broadwebd extensions. It uses
the word plugin for the registry unit, but the initial implementation does not
load dynamic native code. Plugins are safe Rust values linked into Slate and
installed, replaced, or removed from a running daemon through explicit registry
methods.

## Names

Use three names consistently:

- Application service: an application-level capability exposed by broadwebd.
  Examples: `http-fetch`, future `shared-files`, future `calendar-sync`.
- Protocol service: a long-lived protocol driver that owns protocol config,
  state, health, and resource policy. Examples: `ipfs`, future `tor`, future
  `i2p`.
- Transport adapter: a concrete route used by an application service to perform
  one kind of network operation. Examples: `direct-http`, `ipfs-gateway`,
  future `ipfs-kubo-rpc`, future `tor-socks`.

Do not use "app" for broadwebd internals. Slate already uses apps for UI
surfaces in `crates/apps`.

## Module Shape

The broadwebd crate should keep these boundaries:

```text
crates/broadwebd/src/
  daemon.rs
  registry.rs
  state.rs
  budget.rs
  health.rs
  http.rs

  services/
    http_fetch.rs

  protocols/
    ipfs/
      config.rs
      service.rs
      gateway.rs
      types.rs

  transports/
    direct_http.rs
```

`daemon.rs` owns the running service instance. `registry.rs` owns plugin
installation, removal, metadata, and dispatch. `services/` owns
application-level APIs. `protocols/` owns protocol lifecycle and state.
`transports/` owns generic adapters that are not specific to one protocol
service.

## Rust Interfaces

Transport adapters implement `TransportPlugin`:

```rust
fn metadata(&self) -> PluginMetadata;

fn fetch_http(
    &self,
    request: &TransportHttpRequest,
    budget: &ResourceBudget,
) -> Result<HttpFetchResponse, BroadwebdError>;
```

Application services implement `ApplicationServicePlugin`:

```rust
fn metadata(&self) -> PluginMetadata;

fn call(
    &self,
    request: ServiceRequest,
    registry: &PluginRegistry,
    budget: &ResourceBudget,
) -> Result<ServiceResponse, BroadwebdError>;
```

The service receives an approved request and uses the registry to select a
transport adapter. It must not make browser policy decisions such as public
gateway fallback, private-window identity reuse, or DNS leak exceptions.

`PluginMetadata` is the discovery contract. Every plugin should expose:

- stable plugin id;
- plugin kind;
- capabilities;
- dependencies;
- privacy boundary;
- resource profile.

Dependency failures must degrade health. They must not silently fall back to a
different network route.

## Runtime Loading

The first hot-loading model is runtime management of built-in safe Rust
plugins:

```text
BroadwebDaemon::install_transport(...)
BroadwebDaemon::install_service(...)
BroadwebDaemon::remove_transport(...)
BroadwebDaemon::remove_service(...)
```

These methods install, replace, or remove plugins without restarting the
daemon. The daemon keeps the same state root, resource budget, and lifecycle.
This is enough for pre-alpha work because built-in plugins can be enabled,
disabled, or replaced as profile settings, tests, and protocol configuration
change.

The current registry replacement is immediate from the caller's perspective.
The in-process daemon does not yet model concurrent in-flight fetches. Before
broadwebd becomes a separate multi-request daemon, plugin replacement should get
request generations or another explicit rule so an in-flight request continues
with the implementation it started with, or is cancelled clearly.

Dynamic external plugins may be considered later, but they should use a process
boundary and IPC contract. Do not load untrusted native shared libraries into
the daemon process.

## HTTP Fetch Service

`http-fetch` is the first application service. It accepts an approved
`HttpFetchRequest`, selects the requested transport adapter, and returns an
HTTP-like `HttpFetchResponse`.

The response boundary is intentionally small:

- final URL;
- status code;
- headers;
- content type;
- body bytes;
- disposition: render HTML or hand to download flow.

Servo should receive only responses approved by browser-core and fetched
through this boundary. Non-HTML bodies should become download records rather
than rendered documents once Slate has a real download manager.

## IPFS Initial Contract

IPFS should be represented as a protocol service, not just as a URL rewrite.

Initial shape:

```text
IpfsService
  owns IpfsConfig
  owns profile-scoped state paths
  reports local gateway health
  registers ipfs-gateway transport
```

`ipfs-gateway` remains a transport adapter used by `http-fetch`. It maps
`ipfs://` and `ipns://` to an explicitly configured local gateway such as
`http://127.0.0.1:8080`. Public gateway fallback must remain disabled unless
browser-core policy explicitly approves it for the current profile and request.

Later IPFS transports can be added behind the same protocol service:

```text
ipfs-kubo-rpc
ipfs-trustless-fetch
ipfs-delegated-routing
```

Pinning, publishing, providing, and reproviding are not part of the initial
fetch contract. They require separate permissions, budget controls, and user
visibility.

## IPC Mapping

The in-process API should map cleanly to future local IPC:

```text
ListPlugins
InstallBuiltInPlugin
RemovePlugin
Fetch
CallService
SubscribeEvents
```

IPC must carry the same policy-approved request types. The daemon process
should not infer policy from raw URLs.

## Testing Contract

Every plugin should have tests that do not require public network access by
default.

Required coverage:

- metadata and dependency health;
- install, replace, and remove behavior without daemon restart;
- request validation and unsupported scheme failures;
- resource-budget failures;
- fixture-backed fetch success;
- download-vs-render disposition.

External network tests must stay ignored by default and be gated by
`SLATE_EXTERNAL_NETWORK_TESTS=1`.
