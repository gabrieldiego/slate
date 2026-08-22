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
  Examples: `http-fetch`, `profile-sync`, future `shared-files`, future
  `calendar-sync`.
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
    profile_sync.rs

  protocols/
    ipfs/
      config.rs
      service.rs
      gateway.rs
      ipns.rs
      pinning.rs
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

`TransportHttpRequest` carries the approved profile id, target URL, and fetch
purpose. The purpose distinguishes top-level navigations from renderer
subresources. The `http-fetch` application service annotates successful
`HttpFetchResponse` values with `FetchRouteInfo`, including the profile id,
selected transport id, selected transport's privacy boundary, and fetch purpose.
Transport plugins should not rewrite that profile context.

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
- disposition: render HTML, hand to download flow, or show an error page;
- optional download record for profile-temporary non-HTML bodies.

Servo should receive only responses approved by browser-core and fetched
through this boundary. Non-2xx responses are response-error pages, not
downloads, even when the body is plain text or binary. Top-level successful
non-HTML navigation bodies become profile-temporary download records under
broadwebd state and are not rendered as web documents. Subresource bodies such
as CSS, JavaScript, images, and fonts must stay resource responses and must not
create user download records. The full downloads UI can later promote, rename,
remove, verify, or persist top-level download records.

## Profile Sync Service

`profile-sync` is the application service for synchronized profile state. It
receives already approved, already encrypted profile sync objects from storage
code and uses broadweb backends to discover peers, transfer objects, retain
objects, publish mutable roots, and resolve those roots.

The initial service contract should cover:

- `PutEncryptedObject`: store opaque encrypted bytes and return a backend object
  id.
- `GetEncryptedObject`: read opaque encrypted bytes by backend object id.
- `RetainObject`: retain a profile sync object according to backend policy.
- `ReleaseObject`: release a retained object when retention allows it.
- `ListRetainedObjects`: list objects retained by the profile sync service.
- `VerifyRetainedObject`: verify local or provider availability.
- `PublishRoot`: publish the current manifest object id to an approved mutable
  root.
- `ResolveRoot`: resolve a profile mutable root to a manifest object id.
- `ListRootCandidates`: list visible competing mutable-root candidates for
  merge/debug paths before storage chooses which signed objects to trust.
- `DiscoverProviders`: find approved devices or providers that may have profile
  sync objects.
- `OpenTransferSession`: establish a direct, relayed, local, or private-network
  transfer session.
- `WatchHealth`: report backend, transfer, retention, and publish status.

Storage and browser-core own profile semantics, merge policy, encryption, and
signatures. broadwebd owns transport policy and protocol mechanics. The service
must not accept plaintext settings or raw SQLite files.

The first concrete IPFS/IPNS backend is `ipfs-kubo-rpc` on loopback. Future
backends can include embedded IPFS, trustless retrieval, delegated routing,
Iroh, Syncthing-style device providers, Tor-reachable home daemons, Tahoe-LAFS
style storage, or another non-IPFS transport, but they must satisfy the same
profile-sync application contract.

Policy requirements:

- Sync writes do not use public gateways.
- Kubo RPC endpoints are loopback-only by default.
- Remote pinning and provider behavior require visible user policy.
- Discovery and relay providers require visible policy because they can learn
  device identifiers, timing, object sizes, or traffic volume.
- Tests can use a fake in-memory backend without external network access.

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
`IpfsConfig::new` keeps that local gateway as the first-choice endpoint, then
adds a bounded public gateway fallback list for IPFS/IPNS requests that fail to
retrieve through the local gateway. The selected gateway is cached after a `200`
response and rotated on later failures, with each request limited to one pass
through the candidate list.

`IpfsConfig::with_public_gateway` makes the selected public gateway the
first-choice endpoint and keeps the hardcoded public gateway list as fallback.
This sends requested CIDs, IPNS names, timing, and client network metadata to
the configured gateway operator.

`BroadwebDaemon::start_default_session` reads `SLATE_IPFS_GATEWAY` and
`SLATE_IPFS_GATEWAY_SCOPE` to support local manual configuration while keeping
the same policy boundary. `SLATE_IPFS_GATEWAY_SCOPE=public` is required before
a non-loopback public gateway is accepted as the first-choice gateway. A public
scope without an explicit gateway uses the default public gateway list.

Public gateway fallback is useful for resource-constrained devices and early
interoperability tests, but it is not a privacy-preserving substitute for a
local node, delegated trustless retrieval, or a private protocol route.
Environment variables are temporary manual controls; profile-scoped Slate
configuration should replace them before production use.

`ipfs-kubo-rpc` is an opt-in local-node transport behind the same protocol
service. `IpfsConfig::with_kubo_rpc` selects it, the service installs the
`ipfs-kubo-rpc` transport, and HTTP-like retrieval maps `ipfs://` and
`ipns://` to Kubo's local `/api/v0/cat` RPC. The endpoint must be a numeric
loopback address so Slate does not resolve a hostname before contacting the
local node.
`BroadwebDaemon::start_default_session` can select this mode with
`SLATE_IPFS_TRANSPORT=kubo-rpc`, using `SLATE_IPFS_KUBO_RPC` when a non-default
numeric loopback endpoint is needed. Kubo RPC selection is mutually exclusive
with gateway policy variables.
For directory-style paths, it retries `<path>/index.html` after a non-success
`cat` response so simple IPFS/IPNS websites can load through a local node. The
fallback reports the effective `ipfs://` or `ipns://` index URL in the response
so renderer base injection keeps relative subresources under the same
directory. This mode is useful for local-node integration and deterministic
tests, but the gateway transport remains the default because gateway semantics
still handle broader web-style responses more completely.

Both IPFS transports use the common HTTP response classification helper. A
specific `Content-Type` header is preserved; generic binary responses may be
classified from the URL path or an HTML-looking body so simple websites render
instead of being marked as downloads.

Later IPFS transports can be added behind the same protocol service:

```text
ipfs-kubo-sync
ipfs-trustless-fetch
ipfs-delegated-routing
```

Pinning, publishing, providing, and reproviding are not part of the initial
fetch contract. Pinning and publishing enter through `profile-sync` as explicit
capabilities because they change network visibility and storage persistence.
Providing and reproviding still require separate permissions, budget controls,
and user visibility.

## Tor Initial Contract

Tor is a protocol service, not a direct HTTP option:

```text
TorService
  exposes protocol-service metadata
  routes tor+http://, tor+https://, and http(s)://*.onion to tor-arti-http
  registers tor-arti-http
```

`tor-arti-http` embeds Arti and produces HTTP-like responses for onion
documents. The registry must ask protocol services before defaulting to
`direct-http`; otherwise `http://*.onion` could leak to ordinary DNS. The
current implementation supports `tor+http://` only. `tor+https://` is reserved
and must fail closed until Slate adds TLS over Arti streams.

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
- fake profile-sync publish, pin, resolve, and backend failure behavior.

External network tests must stay ignored by default and be gated by
`SLATE_EXTERNAL_NETWORK_TESTS=1`.
