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

`TransportHttpRequest` carries the approved profile id and target URL. The
`http-fetch` application service annotates successful `HttpFetchResponse`
values with `FetchRouteInfo`, including the profile id, selected transport id,
and the selected transport's privacy boundary. Transport plugins should not
rewrite that profile context.

Protocol services implement `ProtocolService`:

```rust
fn metadata(&self) -> PluginMetadata;

fn install_plugins(&self, registry: &mut PluginRegistry) -> Vec<PluginInstallReport>;

fn http_transport_for_url(&self, url: &Url) -> Option<Result<String, BroadwebdError>>;
```

Protocol services own protocol configuration and install the concrete transport
adapters they need. They may route an approved URL to a transport adapter, but
they still must not decide browser policy.

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

The application service receives an approved request and uses the registry to
select a transport adapter. If the request does not name a transport, the
registry resolves one from the URL scheme. Ordinary `http` and `https` resolve
to `direct-http`; `ipfs` and `ipns` resolve through the registered IPFS protocol
service. The service must not make browser policy decisions such as public
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
BroadwebDaemon::install_protocol_service(...)
BroadwebDaemon::remove_transport(...)
BroadwebDaemon::remove_service(...)
BroadwebDaemon::remove_protocol_service(...)
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
`HttpFetchRequest`, resolves or uses the requested transport adapter, and
returns an HTTP-like `HttpFetchResponse`.

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
  exposes protocol-service metadata
  routes ipfs:// and ipns:// to the selected IPFS transport
  registers ipfs-gateway or ipfs-kubo-rpc transport
```

`ipfs-gateway` remains a transport adapter used by `http-fetch`. It maps
`ipfs://` and `ipns://` to an explicitly configured gateway. The default and
normal constructor use a local gateway such as `http://127.0.0.1:8080`.
`IpfsConfig::with_public_gateway` enables public gateway retrieval only when
browser-core policy explicitly chooses that mode for the current profile and
request. Public gateway fallback must remain disabled by default. The current
`IpfsConfig::new` accepts only loopback HTTP(S) gateways, so public gateway
retrieval cannot be enabled accidentally through default construction.

`BroadwebDaemon::start_default_session` reads `SLATE_IPFS_GATEWAY` and
`SLATE_IPFS_GATEWAY_SCOPE` to support local manual configuration while keeping
the same policy boundary. `SLATE_IPFS_GATEWAY_SCOPE=public` is required before
a non-loopback public gateway is accepted. A public scope without an explicit
gateway is rejected.

Public gateway mode sends requested CIDs, IPNS names, timing, and client
network metadata to the configured gateway operator. It is useful for
resource-constrained devices and early interoperability tests, but it is not a
privacy-preserving substitute for a local node, delegated trustless retrieval,
or a private protocol route.

`ipfs-kubo-rpc` is an opt-in local-node transport behind the same protocol
service. `IpfsConfig::with_kubo_rpc` selects it, the service installs the
`ipfs-kubo-rpc` transport, and HTTP-like retrieval maps `ipfs://` and
`ipns://` to Kubo's local `/api/v0/cat` RPC. The endpoint must be loopback.
`BroadwebDaemon::start_default_session` can select this mode with
`SLATE_IPFS_TRANSPORT=kubo-rpc`, using `SLATE_IPFS_KUBO_RPC` when a non-default
loopback endpoint is needed. Kubo RPC selection is mutually exclusive with
gateway policy variables.
For directory-style paths, it retries `<path>/index.html` after a non-success
`cat` response so simple IPFS/IPNS websites can load through a local node. This
mode is useful for local-node integration and deterministic tests, but the
gateway transport remains the default because gateway semantics still handle
broader web-style responses more completely.

Both IPFS transports use the common HTTP response classification helper. A
specific `Content-Type` header is preserved; generic binary responses may be
classified from the URL path or an HTML-looking body so simple websites render
instead of being marked as downloads.

Later IPFS transports can be added behind the same protocol service:

```text
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
