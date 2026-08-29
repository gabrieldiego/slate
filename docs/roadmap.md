# Slate Roadmap Notes

## Browser Chrome

The browser chrome now has a deterministic headless verification loop through
`make chrome-verify`. It renders canonical browser states, saves full-window
screenshots and stable element crops, and writes `report.json` with crop metrics,
monitor pass/warn/fail status, and automated-review findings. Keep this loop
small enough to run with constrained build memory and one Cargo job.

Already covered:

- Full, loading, and toolbar-hover chrome states are rendered headlessly from the
  real egui chrome path.
- Stable crops cover the rail buttons and icons, Web rail tab title/close
  regions, fallback tab icons, toolbar navigation icons, address field controls,
  toolbar menu, privacy shield, and footer status.
- Deterministic checks cover pixel bounds, detail/dark-pixel presence, separator
  artifacts, selected-rail edge affordance, and toolbar-control density. Manual
  review notes remain attached where visual judgment is still required.
- Previously fixed defects stay represented in the fixture where their UI still
  exists: the Home rail icon, selected rail edge affordance, fallback tab icon
  identity, tab title clipping before the close-button reservation, tab close
  art, toolbar vector-icon smoke coverage, and Back/Reload hover-button
  alignment. The old app-title divider issue was retired with the top banner.
- Rail apps render their page bodies as internal HTML under `slate://`. Calendar
  now has an initial `slate://calendar` mock page and is selected from the rail
  as a singleton app page, outside the Web tab preview list.
- Chat now has an initial `slate://chat` aggregator mock for SMS, WhatsApp, and
  future provider adapters. The older `slate://messages` route remains an alias
  while product terminology moves to Chat. Its first sync projection stores
  conversation metadata only; message contents and provider secrets stay out of
  replicated settings payloads.
- Browser-core no longer emits hardcoded app page bodies for rail apps; it maps
  app selection to internal `slate://` addresses and lets the renderer path load
  the page.

Next:

- Diagnose why vector-rendered icons break in the current UI rendering path
  before replacing the temporary raster and alpha-mask assets. Once the rendering
  failure is understood and documented, revisit rail and toolbar icons in vector
  format so they scale cleanly on high-resolution displays.
- Add approved reference-image comparison for the stable crops after the current
  raster theme settles enough that intentional polish changes are less frequent.
- Extend automated review where metrics can stay robust, especially for hover
  shade/glyph centering, control contrast, text clipping, and divider-like
  artifacts.
- Treat the intentional teal accent color as design direction, not as a visual
  regression. The verification process should focus on theme consistency,
  alignment, icon identity, and geometry unless a color change is explicitly
  called out as unintended.
- Keep a manual/image-recognition review step for issues that are visible to a
  human but hard to capture with exact thresholds. The automated loop should
  produce the crops and metadata needed for review instead of relying only on
  full-window screenshots.

## Developer Environment

- Add a Makefile setup target that prepares the local development environment
  without changing the rest of the machine. It should install or verify the
  pinned Rust toolchain under `./.rustup` and use `./.cargo` for project-local
  Cargo state.
- Make the main build targets consistently use the local Rust environment when
  it exists, so `make shared-release` does not fall back to `~/.rustup` on a
  small root filesystem.
- Add a check target for required host tools and libraries. Missing tools should
  be reported with install guidance, while setup actions that download or
  install software should stay explicit.
- Keep memory-heavy browser builds conservative by default. Shared release
  builds should continue to prefer one Cargo job unless a developer opts into
  more parallelism.

## Trust And Name Resolution

Slate should treat name resolution, certificate validation, and routing as one
coherent trust decision rather than separate incidental library calls. The goal
is not to claim perfect zero trust, but to avoid trusting any resolver, proxy,
exit relay, gateway, operating-system path, or certificate store more than the
selected browsing mode requires.

- Define a Slate-owned trust policy layer that records which roots are trusted
  for HTTPS, where they came from, how they are updated, and whether Servo page
  loads and Slate-owned `reqwest` fetches use the same policy. Today these paths
  can differ between platform roots and WebPKI/Mozilla roots, so production
  readiness requires a deliberate, auditable choice.
- Make DNS resolution part of the routing plan. A route should explicitly choose
  `SystemDnsAllowed`, `EncryptedDnsStrict`, `ObliviousDnsStrict`,
  `TorExitRemoteDns`, `TorDoH`, `DistributedLocalNameNode`, or `NoDns` instead
  of allowing library defaults or the OS resolver to decide silently.
- Add a strict DNS-leak policy: no local DNS fallback for private routes, Tor
  routes, distributed protocol names, `.onion`, or any mode whose user-facing
  promise depends on hiding lookup metadata from the local network.
- Investigate local DNSSEC validation for signed conventional DNS zones. DNS
  answers may arrive through an untrusted resolver, cache, Tor circuit, ODoH
  target, or distributed mirror, but Slate should be able to reject tampered
  signed records by validating the chain from a pinned root trust anchor.
- Preserve practical usability by making validation failures explainable and
  recoverable only through explicit user action. Strict private modes should
  fail closed; normal browsing may offer compatibility fallbacks only when the
  privacy and authenticity consequences are visible.
- Support Tor-style privacy boundaries without depending on Tor exits for all
  trust. For clearnet Tor browsing, prefer hostname-aware proxying or DoH over
  Tor so local DNS is never used; use HTTPS, HTTPS-Only behavior, ECH where
  available, and certificate validation to protect the endpoint.
- Treat self-authenticating and distributed names as first-class routing cases.
  `.onion` names should use onion-service authentication instead of DNS, IPFS
  CIDs should use content identity instead of DNS identity, and systems such as
  Namecoin, Handshake, ENS, or IPNS should be integrated only through explicit
  adapters with documented trust, privacy, update, and failure behavior.
- Add tests that prove resolver policy is enforced: no OS DNS calls in strict
  private routes, no fallback from Tor or ODoH to system DNS, DNSSEC-bogus
  answers rejected in strict modes, and HTTPS certificate errors never bypassed
  except through a visible site-specific exception.

## Profile State

Persist Slate-owned user state in `slate-settings.db` whenever it is small,
structured, profile-specific, and useful across restarts.

Already covered:

- Settings, including chrome zoom and configurable key bindings.
- First-run bookmarks and the visible home bookmark slots.
- Home bookmark favicons and other small binary blobs.
- Browsing history URL/title/visit metadata from Servo history callbacks.
- Cookie schema, though live HTTP cookies are still owned by Servo today.
- Downloads metadata now has a `slate-settings.db` materialized table and
  sync-domain JSON projection for URL, route, transport, filename, MIME type,
  byte count, and integrity metadata. File bytes and local paths stay outside
  the replicated settings payload.
- Calendar events now have a local-first `slate-settings.db` materialized table
  and sync-domain JSON projection with tombstones. Calendar sync stays disabled
  by default because event titles, notes, locations, recurrence, and reminders
  are sensitive and must only leave the device inside encrypted profile-sync
  objects.
- Contact cards now have a local-first `slate-settings.db` materialized table
  and sync-domain JSON projection with tombstones. Contacts sync stays disabled
  by default because names, email addresses, phone numbers, notes, and avatar
  references are sensitive profile data.

Backlog:

- Downloads: persistent file records, local saved-path handling, status,
  timestamps surfaced in the UI, failure reason, and promotion rules for
  temporary downloads. Download files should remain normal files; replicated
  settings payloads should continue to exclude local paths and file bytes.
- Session restore: open windows, tab order, active tab, singleton internal tabs,
  last URL/title per tab, and eventually back/forward history when the rendering
  boundary exposes enough state.
- Protocol configuration: enabled adapters, IPFS gateway policy and last working
  gateway, Kubo endpoint, Tor/Arti configuration, per-protocol budgets, and
  public-gateway consent.
- Protocol health metadata: recent failures, selected transport, last checked
  time, and user-visible status. Heavy protocol caches and Tor/IPFS node state
  should stay outside SQLite with only indexes or configuration in the profile
  database.
- Permissions and site settings: clipboard, downloads, storage, cookies,
  pop-ups, public gateway routing, Tor routing, and future protocol prompts.
- Privacy identity state: normal/private profile separation, containers,
  routing mode preferences, DNS leak policy, and fingerprinting controls.
- Full cookie integration: replace Servo's current `cookie_jar.json` path only
  after reads, writes, expiry, clearing, and private-mode behavior can flow
  through Slate storage without duplicated state.
- Browser preferences beyond zoom: startup behavior, home page choice, sidebar
  visibility/order, bookmark display policy, search engine choice, suggestions
  policy, and theme once multiple themes exist.
- Bookmark management beyond home slots: folders, ordering, tags or notes,
  deletion state, and migration behavior for default bookmarks.
- Security exceptions: certificate exceptions, mixed-content exceptions, and
  similar high-risk state only after a dedicated privacy/security design.

Priority order:

1. Downloads table and UI wiring.
2. Session restore.
3. Protocol configuration and consent.
4. Permissions and site settings.
5. Cookie integration.

## Profile Sync

_Current focus: fully validating safe profile sync locally before real-internet
broadweb testing._

Current goal:

- Fully validate Slate distributed profile synchronization locally before
  real-internet testing. Keep `slate-settings.db` as local-first encrypted
  syncable state, implement the runtime paths needed for user-facing profile
  sync, model chosen broadweb protocol behavior through local-only
  deterministic fixtures that mirror real adapter boundaries, avoid live
  network dependencies in automated tests, commit each small step, and
  revalidate profile storage, rail app, chrome, broadwebd, and protocol
  boundaries to prevent regressions.

Architecture note:

- [Profile Sync Over Broadweb Protocols](architecture/profile-sync.md)

Current baseline:

- broadwebd can retrieve `ipfs://` and `ipns://` resources through the local
  IPFS gateway and an opt-in Kubo RPC fetch path.
- Slate stores local settings and bookmarks in profile-owned SQLite state.
- `slate-settings.db` is the first target for local-first distributed profile
  state and typed sync change streams.
- `slate-settings.db` now records typed settings values, changes, revisions,
  app sync domains, known sync devices, per-instance local sync device ids, and
  idempotent incoming setting-change application.
- App sync domains for Settings, Bookmarks, Downloads, Calendar, Contacts,
  Chat, Files, and Storage are seeded into `slate-settings.db` with explicit
  privacy classifications. Low-risk and metadata domains are enabled by
  default; sensitive and content-bearing app domains are present but opt-in,
  and default seeding preserves a user's stored enable/disable choice.
- The rail app registry now maps visible apps to distinct sync domains:
  Web owns the Bookmarks domain and Settings owns the Settings domain. Storage
  remains a seeded future domain until its rail app surface exists.
- Home bookmark slot saves now append structured JSON text changes in the
  Bookmarks sync domain. First-run default bookmark seeding still updates only
  local bookmark rows, so new profiles do not publish seed noise as user
  bookmark changes. Trusted incoming bookmark-slot changes now materialize into
  the local bookmark rows during profile-sync apply, and existing bookmark
  removals emit slot tombstones that delete stale rows on receiving devices.
- Chat conversation metadata now has a local-first `slate-settings.db`
  materialized table and sync-domain JSON projection with tombstones. The
  initial projection tracks provider ID, provider thread ID, display name,
  avatar reference, last-message timestamp, unread count, archive state, and
  mute state. Message bodies, SMS/WhatsApp credentials, provider tokens, and
  attachment bytes remain outside replicated settings payloads.
- Files metadata now has a local-first `slate-settings.db` materialized table
  and sync-domain JSON projection with tombstones. The initial projection tracks
  sync-set membership, parent entry, name, entry kind, content object reference,
  MIME type, size, modified time, integrity, and retention policy. File bytes,
  local paths, and per-device availability stay out of replicated settings
  payloads until heavier object-transfer backends are wired.
- The Files rail app should present a normal file-browser surface first:
  folders, files, search, sort, details, sync sets, and clear local/remote
  availability states. Users should not need to know whether backing data came
  from IPFS, cache, a retained object, or another broadweb provider during
  everyday file browsing; provider/cache/IPFS details belong in settings,
  diagnostics, or advanced per-file metadata.
- Storage provider metadata now has a local-first `slate-settings.db`
  materialized table and sync-domain JSON projection with tombstones. The
  initial projection tracks provider kind, display name, endpoint reference,
  broadweb role flags, quota hints, retained-object limits, pinning policy, and
  enabled state. Provider credentials, private keys, local daemon paths, live
  health, and per-device availability stay local to runtime or secret storage.
- The local broadwebd profile-sync fixture now carries typed app-domain
  metadata through the encrypted manifest path between two local
  `slate-settings.db` instances. The full-snapshot regression covers Calendar,
  Chat, Contacts, Downloads, Files, and Storage projections over the in-process
  fixture network; encrypted tail regressions cover Calendar, Chat, Contacts,
  Downloads, Files, and Storage without loopback sockets.
- The `slate-profile-sync` runtime bridge now also has full-snapshot
  publisher/receiver regressions for typed Calendar, Chat, Contacts, Downloads,
  Files, and Storage metadata. The tests enable those opt-in domains on the
  publisher, publish signed encrypted device-head snapshots through broadwebd's
  in-process fixture, verify the receiver materializes typed rows, verify
  post-snapshot update tails for Calendar, Chat, Contacts, Downloads, Files,
  and Storage, verify tombstone snapshots for Calendar, Chat, Contacts,
  Downloads, Files, and Storage, and verify
  Calendar/Chat/Contacts/Downloads/Files/Storage post-snapshot tombstone tails
  delete stale typed rows on the receiver.
- Local settings/app-domain publishing now filters outgoing snapshots and tail
  manifests through the enabled app sync-domain table. Disabled domains can
  still be used as local typed state, but they are not published to broadweb
  profile-sync objects until explicitly enabled. The compaction target is also
  selected from enabled domains, so disabled-domain churn does not force
  outgoing snapshots or manifest tails. The profile-sync publisher now composes
  enabled-domain event feeds directly instead of loading all app-domain
  settings changes and filtering disabled domains in memory. Runtime bridge
  coverage now includes disabled typed Chat metadata, proving sensitive
  app-domain rows stay local across both snapshot and tail publish checks while
  the domain is disabled.
- broadwebd has a protocol-neutral `profile-sync` application service with a
  local in-memory preview backend. Unit tests cover object transfer, retention,
  mutable root publish/resolve, provider discovery, per-object transfer
  budgets, and two local `slate-settings.db` files syncing one setting through
  fixture bytes.
- broadwebd now exposes a public `BroadwebdClient` trait over the same
  in-process request/response boundary used by `BroadwebDaemon`. This gives
  Slate callers and future IPC clients a stable boundary for HTTP fetches,
  profile-sync requests, health, status, and download listing without exposing
  protocol internals.
- The broadwebd client boundary now includes a single
  `dispatch_service_request` method over `ServiceRequest` and `ServiceResponse`
  envelopes. The typed HTTP and profile-sync client methods are convenience
  wrappers over that dispatch path, so future IPC clients can implement one
  command surface while Slate retains typed call sites.
- The same service request/response envelopes now round-trip through JSON in
  broadwebd unit tests. This does not choose the final IPC transport yet, but it
  keeps the API boundary framed-message-ready instead of relying on
  process-local object references.
- broadwebd now has a bounded JSON service-frame codec around those envelopes.
  It rejects oversized incoming frames before JSON parsing and rejects oversized
  encoded frames while writing, giving local fixtures and future IPC clients an
  explicit memory boundary without opening sockets or choosing the final IPC
  transport.
- broadwebd now has a socketless framed-client adapter that implements
  `BroadwebdClient` by round-tripping service requests and responses through the
  bounded frame codec before dispatch. This lets future profile-sync and
  browser-core tests validate IPC-shaped byte boundaries without loopback ports.
- `slate-profile-sync` has started consuming that boundary directly: its
  source, publisher, runner, and scheduler bridge wrappers now hold
  `BroadwebdClient` trait objects instead of concrete daemon references.
- `slate-profile-sync` now has an envelope-only `BroadwebdClient` regression
  that implements `dispatch_service_request` without a `BroadwebDaemon`. This
  proves the profile-sync source, publisher, and runner bridges can work through
  the future IPC-shaped service envelope while the test remains fully
  in-process.
- `slate-profile-sync` now also runs its source, publisher, runner, and
  scheduler-construction bridge checks through broadwebd's socketless framed
  client. That pushes the byte-framed boundary up to the runtime glue without
  opening loopback ports or choosing the final IPC transport.
- A two-device settings sync cycle now publishes and applies a setting through
  framed broadwebd clients over the local fixture network. This proves a real
  runtime settings sync path works across the socketless byte boundary, not only
  the lower-level bridge wrappers.
- Selected retention-provider settings sync now also runs through framed
  broadwebd clients for both the local device scheduler and provider handles,
  so provider retention and availability calls are covered across the same
  socketless byte boundary.
- broadwebd now also has an opt-in TCP service-frame client and
  `slate-broadwebd-net-probe` utility for manual LAN smoke tests. This keeps the
  same bounded service request/response envelopes, but exercises them across a
  real network link for put/publish/resolve/get/retain/root-health profile-sync
  operations. The companion `scripts/profile-sync-lan-smoke.sh` stages the
  probe through an SSH target supplied at runtime, caps the remote process at
  256 MiB by default, uses a temporary remote state root, and removes the remote
  artifacts after the smoke run.
- broadwebd now has a minimal profile-sync peer-discovery contract. UDP solicit
  and advertisement messages carry a network id, node id, provider id,
  service-frame TCP endpoint, capabilities, and sequence number. The opt-in
  local socket fixture swaps in loopback UDP/TCP sockets and proves discovery
  can find a peer before running the profile-sync service-frame smoke. The
  companion `scripts/profile-sync-p2p-lan-smoke.sh` can stage a temporary peer
  on an SSH target, answer multicast discovery, let the local node discover the
  peer, and then run the same put/publish/resolve/get/retain/root-health
  sequence through the discovered endpoint under 256 MiB runtime caps.
- The in-memory `LocalProfileSyncFixture` is no longer re-exported from
  broadwebd's normal root API. Local preview helpers that still need the
  deterministic model import it through `slate_broadwebd::test_fixtures`, so the
  user-facing preview path remains clearly backed by a fixture-layer socket
  substitute rather than a runtime protocol implementation.
- The local profile-sync fixture can now mark arbitrary known provider IDs
  online or offline, not only device-shaped providers. Unknown provider IDs are
  rejected so local protocol tests do not silently model the wrong peer.
- Object-transfer and mutable-root propagation delays can now target arbitrary
  known provider IDs as well as device-shaped providers. This lets socketless
  tests model delayed custom availability, transfer, and root providers without
  inventing fake device identities.
- broadwebd's simulated HTTP gateway and Kubo RPC fixtures now use test-only
  `slate-fixture-http://` and `slate-fixture-kubo://` schemes that resolve
  through the `InProcessBroadwebNetwork` fixture layer inside the test process
  instead of binding loopback ports. Missing simulated fixtures fail as
  internal fixture errors instead of falling through to a real socket, DNS
  lookup, public gateway, or local daemon. The daemon fetch resolver now honors
  these synthetic HTTP fixture URLs whenever broadwebd is built with the
  `test-fixtures` feature, so downstream fixture tests stay in process too.
- `InProcessBroadwebNetwork` now exposes fixture-only broadwebd registries and
  per-device and availability-provider daemon constructors. These registries
  install an in-process HTTP transport that handles only
  `slate-fixture-http://` URLs and rejects ordinary `http://` or `https://`
  requests, while profile-sync registries share one in-memory simulated network
  state. The synthetic HTTP and Kubo fixture URLs are scoped to the creating
  `InProcessBroadwebNetwork`, and the fixture daemon constructors reject
  loopback gateway or RPC endpoints so simulated tests cannot drift back to
  local sockets.
- Fixture transports now advertise `socketless-fixture` in broadwebd health
  metadata, and synthetic Kubo RPC fixture fetches resolve through the
  in-process registry before constructing any real HTTP client. This keeps the
  default profile-sync and rendering fixture loops independent from loopback
  listeners, firewall state, DNS, public gateways, or escalation prompts.
- The profile-sync boundary gate now rejects direct fixture-model or fixture
  registry calls from the Kubo protocol implementation and broadwebd
  profile-sync service. Local models stay behind transport executor shims, so
  the adapters keep the same request/response shape expected from real Kubo,
  IPFS/IPNS, and future broadweb backends. Kubo fixture shims are exported
  only through broadwebd's `test_fixtures` module now, not through the normal
  `protocols::ipfs` API.
- The opt-in Kubo RPC endpoint validator and local IPFS gateway validator now
  accept only numeric loopback addresses, plus synthetic in-process fixture URLs
  in tests. Hostname-shaped endpoints such as `localhost` are rejected before
  any HTTP client or resolver can be involved.
- Kubo RPC now exposes pure profile-sync endpoint builders for encrypted object
  add, pin, unpin, pin-status, IPNS publish, and IPNS resolve. The helpers
  reuse the same numeric-loopback or synthetic-fixture endpoint validation and
  give the future backend client a tested RPC surface before any networked sync
  implementation is wired in.
- Kubo profile-sync response parsing now extracts object ids from add,
  pin-status, IPNS publish, and IPNS resolve responses, and fails malformed
  local-node data before it can update a profile-sync root or retained-object
  state.
- Kubo profile-sync now has a socketless request planner that maps broadwebd's
  profile-sync verbs to Kubo add, pin, unpin, pin-status, IPNS publish, and
  IPNS resolve RPC calls. This gives the future backend client a typed boundary
  before it is allowed to issue local-node requests.
- The in-process Kubo fixture can now execute planned profile-sync RPC requests
  and return raw fixture responses under the profile-sync object budget. This
  keeps backend-client tests socketless while preserving the existing HTTP
  response budget for browsing-style Kubo fetches.
- The Kubo profile-sync fixture client can now put one encrypted profile object
  through that socketless add path, enforce the profile-sync object budget
  before issuing the simulated request, require a successful Kubo status, and
  return the parsed object id. This is still a local fixture backend, not a
  live Kubo HTTP client.
- The same Kubo fixture client now covers the retained-object lifecycle for
  encrypted profile-sync objects: retain through `pin/add`, verify recursive
  retention through `pin/ls`, and release through `pin/rm`. The tests still run
  entirely inside the in-process fixture and do not open loopback sockets.
- Kubo mutable-root fixture calls now publish and resolve profile-sync roots
  through the socketless IPNS `name/publish` and `name/resolve` paths. Publish
  rejects a Kubo response that points at a different object id than the one Slate
  requested, so a mismatched root cannot be accepted by the backend client.
- broadwebd's `profile-sync` service can now be constructed with a socketless
  Kubo fixture backend. Normal `ProfileSyncRequest` calls for put, get, retain,
  verify, release, publish, resolve, and provider discovery are translated into
  the same in-process Kubo RPC fixture calls. Object fetch uses Kubo `cat` under
  the profile-sync object budget, giving the runtime service contract a
  protocol-shaped backend path without opening sockets.
- The Kubo-backed fixture service now treats object fetch as authoritative from
  the Kubo-shaped transport path: `GetEncryptedObject` returns the bytes from
  the internal `cat` request or stateful Kubo model, not a hidden local upload
  cache. Upload bookkeeping remains local service metadata for health and
  retention policy only.
- Socketless broadweb models must stay behind transport shims. Protocol clients
  should keep building and validating the same HTTP/RPC requests and responses
  they would use on the real web; fixture models only replace socket I/O with
  process-local simulated behavior.
- Kubo profile-sync verbs now execute through an explicit RPC executor trait.
  The protocol methods remain production-shaped (`put`, `cat`, `pin`,
  `name/publish`, and `name/resolve`), while the in-process fixture implements
  the executor and rejects non-fixture URLs before any socket can be touched.
- Kubo IPFS content fetches now keep the same boundary: the normal
  `IpfsKuboRpcTransport` is fixture-blind and always represents local Kubo
  HTTP, while `InProcessBroadwebNetwork` installs a fixture-side transport
  wrapper that reuses the same Kubo `cat` request builder through the internal
  executor shim.
- broadwebd now also has a real HTTP Kubo profile-sync executor and service
  constructor. Production Kubo profile-sync uses the same request builders,
  response parsers, role checks, and resource budgets over HTTP; socketless
  tests swap only the executor with the internal transport shim.
- The Kubo profile-sync service no longer contains a fixture transport enum or
  a direct internal-shim branch. It stores an injected executor factory: runtime
  construction supplies the reqwest HTTP executor, while
  `InProcessBroadwebNetwork` supplies the socketless fixture executor from the
  fixture layer after validating same-network synthetic endpoints.
- The Kubo/IPNS profile-sync model now inherits `InProcessBroadwebNetwork`
  fixture capacity limits. The socketless model rejects new encrypted objects,
  and IPNS names after the configured object/root bounds, and the Kubo-backed
  profile-sync service preflights the same limits before issuing a fixture RPC.
  This keeps multi-device local simulations bounded while still exercising
  Kubo-shaped `add`, `pin`, and `name/publish` requests.
- The default broadwebd registry remains local-only, but callers can now opt in
  to a local Kubo HTTP profile-sync backend through an explicit registry
  constructor. That constructor replaces the local preview profile-sync service,
  rejects non-loopback Kubo endpoints up front, and advertises
  `profile-sync/kubo-http` rather than fixture capabilities.
- `BroadwebDaemon::start_default_session` now reads an explicit profile-sync
  runtime backend config. With no profile-sync backend settings it keeps the
  local in-memory service; `kubo-rpc` selection or a profile-sync Kubo endpoint
  opts into the loopback-only Kubo HTTP backend with deterministic parser tests.
- Profile-sync runtime selection is now fixture-blind: legacy `fake` and
  `local-fake` backend aliases, in-process fixture names, and
  `slate-fixture-*` endpoint refs are rejected from runtime config. Synthetic
  broadweb models remain available only when the deterministic fixture harness
  injects transport/executor shims under the same protocol request builders.
- The same Kubo fixture service now records retained objects and published roots
  in its in-process profile-sync state after the corresponding Kubo RPC fixture
  calls succeed. Retained-object listing, provider health, root candidates, and
  root health now work for that backend without adding sockets or external
  services. A release/unpin through the Kubo-shaped executor also clears the
  retained-object view and makes root health degrade when the newest root no
  longer meets the local retention policy.
- `InProcessBroadwebNetwork` now has Kubo profile-sync registry and daemon
  helpers that install the socketless Kubo-backed `profile-sync` service only
  when the Kubo fixture URL belongs to the same simulated network. Downstream
  settings-sync tests can use those helpers without manually assembling a
  registry or opening loopback ports.
- `InProcessBroadwebNetwork` now also exposes a stateful socketless Kubo
  profile-sync model. The protocol adapter still builds normal Kubo RPC URLs
  and parses normal Kubo JSON responses, while the fixture executor delivers
  those requests to a deterministic in-process model for `add`, `cat`,
  `pin/add`, `pin/rm`, `pin/ls`, `name/publish`, and `name/resolve`. Queued
  Kubo responses remain available for explicit error and malformed-response
  tests.
- `slate-profile-sync` now verifies its `BroadwebdProfileSyncPublisher` and
  `BroadwebdProfileSyncObjectSource` bridge against the stateful Kubo profile-sync
  fixture daemon: dependency objects, retained root publish, root resolve, and
  object fetch all flow through Kubo-shaped RPC requests handled by the
  in-process model.
- The same bridge now has a two-daemon stateful Kubo/IPNS fixture test: one
  daemon publishes retained objects and a root, while a second daemon resolves
  the root and fetches the object bytes through the shared in-process model.
  This keeps the current cross-device simulation socketless while exercising a
  protocol-shaped shared backend instead of the writer daemon's local cache.
- The Kubo-backed settings scheduler happy path now uses the same stateful
  Kubo/IPNS fixture model. Published snapshot, manifest, and local device-head
  objects get model-derived fixture CIDs, retention is verified through Kubo
  `pin/ls`, and settings root storage no longer depends on pre-scripted object
  identifiers.
- The stored-provider scheduler also has a first Iroh-shaped fixture model for
  `iroh-node:<node>` endpoint refs. The run materializes the selected provider
  through the caller-supplied socketless protocol materializer, verifies the
  preview path leaves `slate-settings.db` roots unchanged, and retains the
  published settings objects through the in-process provider without loopback
  sockets or a live Iroh network. The same checkpoint now covers a missing
  socketless materializer handle: selected Iroh endpoints remain blocked by
  provider quorum before publishing or mutating sync roots. It also models an
  Iroh-like peer that is discoverable while transfer is delayed: the scheduler
  surfaces a retention error, the provider retains no bytes, and a later run
  succeeds after the fixture releases the transfer path. A live-transfer-only
  Iroh-shaped peer is still rejected for durable retention when broadwebd health
  does not advertise the availability role, even if local stored metadata claims
  it can retain data. The same Iroh-shaped materialized-provider path now also
  inherits `InProcessBroadwebNetwork` fixture capacity limits: a bounded local
  publish can complete, then provider retention fails before the materialized
  provider grows the shared in-process object store beyond its configured
  object budget.
- Internal broadweb models must remain socket substitutes, not protocol
  implementations. IPFS/IPNS, Iroh, Tor, I2P, and future adapters should keep
  building their normal routing plans, requests, responses, parsers, privacy
  boundaries, and provider-role checks as if they were talking to the real web.
  The deterministic fixture layer only swaps the socket transport or daemon
  communication with in-process modeled behavior such as delayed discovery,
  unavailable transfer, stale roots, or malformed protocol responses.
- IPFS gateway fixtures now follow that boundary too: runtime gateway
  validation stays strict HTTP(S), and socketless tests inject a prevalidated
  local gateway endpoint from `InProcessBroadwebNetwork`. The IPFS gateway
  adapter no longer branches on synthetic fixture schemes or advertises
  fixture-specific metadata; the injected HTTP transport owns the no-socket
  simulation behavior.
- IPFS gateway fetching now also uses an explicit HTTP executor boundary. The
  default gateway transport calls the real-network HTTP helper, while
  `InProcessBroadwebNetwork` installs a fixture transport wrapper whose
  executor resolves only same-network synthetic HTTP fixture URLs. This keeps
  gateway fallback, service-worker bootstrap detection, route metadata, and
  response parsing on the same path a real gateway uses while swapping only
  socket I/O in local tests.
- The ambiguous shared HTTP fetch helper was removed. Default/direct HTTP
  fetching now calls `fetch_http_url_over_network` and rejects synthetic
  fixture schemes as unsupported, while in-process HTTP transports call the
  explicit internal fixture fetch helper after same-network validation.
- Default registry and direct HTTP transport are now fixture-blind too.
  `InProcessBroadwebNetwork` installs a fixture-only protocol service that
  maps same-network synthetic HTTP fixture URLs to the in-process fixture
  transport; the default registry has a regression proving fixture URLs do not
  resolve without that fixture protocol service.
- Profile-sync protocol materializer policies now mark whether selected
  provider endpoints are handled by real runtime adapters or by local
  deterministic simulation. The scheduler still consumes typed multiaddr and
  deferred-protocol materialization requests either way; fixture models remain
  a transport boundary choice, not protocol logic.
- `slate-profile-sync` no longer enables broadwebd test fixtures for normal
  consumers. The settings-page local preview flow opts into a
  `local-preview-fixtures` feature explicitly, so simulated broadweb models can
  back user-facing local trials without intermingling with production protocol
  adapter code or default dependency graphs.
- Socketless profile-sync root health now reports which fresh provider IDs can
  serve the latest root object, and separately names stale or offline providers
  that still hold that object. This lets schedulers and UI distinguish missing
  bytes from delayed transfer, stale rendezvous, and offline availability in
  deterministic local tests before real broadweb transports are enabled. The
  profile-sync runner now exposes structured root-object provider issues for
  delayed, stale, and offline holders, with regressions proving those states
  propagate through the scheduler-facing `settings_sync_health` surface. Local
  preview reports carry string-based issue summaries, and the settings page
  exposes them as a compact sync-issue status plus issue details. Retention
  provider selection and stored provider metadata issues are now carried as the
  same kind of protocol-neutral local preview summaries, without importing
  fixture models. Local readiness now also carries app sync domain records from
  `slate-settings.db`, and the settings preview shows the enabled domains so
  Settings, Bookmarks, Downloads, and future rail-app domains can be checked
  before running broader sync trials. The same readiness surface now carries
  storage provider records and the preview shows active retention-capable
  providers from `slate-settings.db`. The settings preview also exposes each
  active provider's stored endpoint reference as metadata, giving local trials
  a visible distinction between fixture, multiaddr, and future adapter
  endpoints without letting the UI inspect fixture model state. Local preview
  sync run reports now also carry a compact selected-endpoint materialization
  summary: fixture-ready providers, pending protocol-provider work, missing
  endpoint metadata, fail-closed endpoints, and whether a protocol materializer
  is required. The settings page renders that summary from protocol-neutral
  report fields. Local preview reports also carry structured retained-object
  issues for objects that a selected provider did not retain or cannot serve
  after a retention attempt, and the settings page includes those issues in the
  compact sync-issue status. Those records remain profile state while protocol
  adapters continue to behave as real-web clients with only their socket or
  daemon transport swapped by local deterministic shims in tests.
- Trusted device-head receives now ignore stale root rollback: if a resolved
  device head points at an older sequence than a locally applied head for the
  same trusted device, Slate leaves the newer settings and device-head roots in
  place. Equal-sequence pulls may still record a missing device-head root after
  the receiver learned the same changes through the shared settings root first.
  The regressions run through the socketless broadwebd path so the sync logic
  sees the same root/object boundary a real protocol adapter will expose.
- Shared `settings/latest` candidate receives now also ignore replayed older
  manifests whose device frontiers are already covered locally. The socketless
  broadwebd regression republishes an older manifest after a newer sequence was
  applied and verifies the receiver leaves the newer shared root in place.
- Corrupt shared-root objects are now covered through the socketless broadwebd
  bridge: when `settings/latest` resolves to available bytes that are not a
  trusted signed encrypted manifest, the receiver fails closed and keeps the
  previous valid root and materialized settings.
- Shared-root objects carrying the wrong content-key id are covered through the
  same bridge. A trusted signer cannot move `settings/latest` with an object
  labeled for a non-active key epoch; the receiver rejects it before mutation.
- Invalid signatures on parseable shared-root objects are covered through the
  same socketless bridge, so available bytes cannot advance `settings/latest`
  unless the trusted-device signature verifies.
- Malformed app-domain payloads in shared-root manifests are covered through
  the same bridge; application fails inside the storage transaction and leaves
  the previous shared root and materialized app state unchanged.
- Shared-root manifests that reference unavailable tail objects are covered
  through the same bridge. The receiver fails at the pull/source boundary and
  leaves the previous shared root, materialized settings, and trusted-device
  sequence unchanged.
- The local profile-sync fixture can now model a provider that still claims a
  retained object after its provider-held bytes disappear. Retained-object
  verification reports `retained: true` with unavailable bytes, and root health
  no longer counts that provider toward retaining-provider quorum until bytes
  are restored through the normal retain path. The profile-sync runner now
  preserves that provider identity as a structured `retained_unavailable`
  root-object provider issue, so settings previews and scheduler diagnostics can
  distinguish missing retained bytes from delayed, stale, or offline providers.
- The same bridge now publishes a real signed encrypted settings tail manifest
  through one Kubo fixture daemon, then verifies the manifest and tail bytes
  fetched by a second daemon through the shared stateful Kubo/IPNS model. The
  verification uses the normal storage sync-object openers instead of
  hand-authored payloads or cached writer-daemon bytes.
- broadwebd's own HTTP fixture unit tests now start daemons with
  `InProcessBroadwebNetwork` fixture registries whenever they consume
  `slate-fixture-http://` URLs. That keeps the production default direct HTTP
  registry from becoming a fixture backdoor while still testing response
  classification, downloads, headers, and budget enforcement entirely in
  process.
- Rendering broadweb smoke fixtures now consume that same `test-fixtures`
  layer, so IPFS/IPNS gateway and Kubo subresource tests record simulated
  requests without starting loopback HTTP servers. The rendering tests now use
  one shared `InProcessBroadwebNetwork` for both fixture URL registration and
  daemon construction instead of socket-shaped local fixture handles.
- `slate-settings.db` can now persist app-domain watcher cursors in
  profile-scoped `sync_state` rows. Cursor writes are monotonic, so stale or
  duplicate watcher batches cannot move a rail app's sync cursor backward.
  Storage also exposes a cursor-backed app-domain settings poll helper that
  initializes missing cursors at the domain head and lets callers advance the
  cursor only after they apply a returned batch.
- Storage now also exposes a typed app-domain poll helper that decodes
  sync-domain JSON payloads before a watcher advances its persisted cursor. A
  malformed app payload fails the poll and leaves the app cursor unchanged, so
  runtime app watchers can avoid acknowledging a batch they did not apply.
- Storage now also has a reusable typed app-domain watcher wrapper. Runtime app
  code can initialize a domain cursor at the current domain head, poll bounded
  decoded batches, and acknowledge the batch only after app-owned apply work
  succeeds. The wrapper also has an apply-and-acknowledge helper that runs an
  app callback first and persists the cursor only when that callback succeeds.
- Storage now also has the same reusable app-domain watcher shape for raw text
  payloads. This lets runtime code such as Chrome settings watch non-JSON
  setting values while still acknowledging the persisted cursor only after the
  app apply step succeeds.
- The `slate-profile-sync` runtime bridge now verifies received typed Calendar,
  Chat, Contacts, Downloads, Files, and Storage metadata is visible through
  those typed app-domain watcher polls after a trusted broadwebd apply. The
  fixture initializes receiver cursors before sync, applies a signed encrypted
  snapshot, then uses the typed watcher apply-and-acknowledge helper so each
  cursor is persisted only after the simulated app callback inspects the
  decoded payload batch. The same watcher path now covers post-snapshot update
  tails for Calendar, Chat, Contacts, Downloads, Files, and Storage, so apps
  can observe incremental metadata changes after acknowledging the snapshot
  batch. Calendar, Chat, Contacts, Downloads, Files, and Storage tombstone
  tails are covered through the same typed watcher path, proving deletions are
  observable before the app advances its cursor.
- The profile-sync bridge can now publish post-snapshot local settings updates
  by reusing the latest retained `slate-settings.db` snapshot object, publishing
  only the new tail changes, moving the settings root, and publishing a fresh
  local device head. The regression test runs two simulated devices through
  `InProcessBroadwebNetwork`, so the incremental handoff stays inside the test
  process without loopback sockets or external protocol services.
- `slate-profile-sync` now has a scheduler-facing local settings head publisher
  that chooses the next local action from `slate-settings.db`: no-op when a
  profile has no changes, full snapshot for the first publish or unpublished
  snapshot state, no-op when the retained snapshot is current, and incremental
  tail publish when changes exist after that snapshot.
- The first bounded local settings sync loop now wraps that publisher and runs
  until an explicit idle state, with a caller-supplied maximum step count. It
  reads the stored local device-head root as the published local frontier, so it
  does not repeatedly publish the same post-snapshot tail and does not re-sign
  remote-device rows as local tail changes.
- The receive side now has a bounded trusted-device settings sync runner. It
  reads the trusted device public-key table from `slate-settings.db`, skips the
  local device, enforces a caller-supplied maximum trusted-device count, and
  pulls each trusted settings device head through broadwebd before applying new
  manifests.
- The broadwebd-backed profile-sync source can now also apply active trusted
  competing settings-root candidates through storage's candidate merge path.
  The in-process regression covers two equal-control devices publishing
  different signed `settings/latest` roots and a third trusted device applying
  both candidates without loopback sockets.
- The low-memory boundary gate now also covers the losing side of that
  equal-control conflict: a lower-tie-break shared-root candidate is fetched,
  verified, retained in `settings_changes`, and left unapplied while the
  materialized setting and watcher events continue to reflect the deterministic
  winner.
- `slate-profile-sync` now exposes a bounded settings sync cycle that preflights
  the trusted-device count, publishes local pending settings first, then pulls
  registered trusted device heads. The in-process two-device fixture covers a
  round trip where one device publishes a full snapshot, the second receives it
  and publishes an incremental tail, and the first receives that tail without
  opening loopback sockets.
- The active-key policy cycle can now include shared settings-root candidate
  application. A receiver can run one bounded cycle that publishes nothing,
  observes no per-device-head updates, applies competing `settings/latest`
  candidates through broadwebd, and reports that shared-root candidate work in
  the cycle result.
- Verified settings manifest application now preserves the manifest, snapshot,
  and tail object ids it consumed. The shared-root cycle exposes those received
  candidate object ids so an availability provider can retain the verified
  objects after a merge and improve shared-root quorum without gaining profile
  write authority.
- The settings sync cycle now validates caller-supplied credentials against
  `slate-settings.db` before touching broadwebd: the requested content-key id
  must be the active key epoch, the active epoch must use the supported content
  encryption algorithm, and the supplied local signer must match the trusted
  public key recorded for the local sync device. Secret key bytes remain outside
  the database.
- `slate-profile-sync` now exposes a read-only settings sync health report that
  combines broadwebd provider health, the shared settings root health, and the
  local device-head root health. The regression test runs through
  `InProcessBroadwebNetwork`, so scheduler-facing health checks stay inside the
  test process and do not open loopback ports.
- Settings sync health reports now also expose structured degradation issues
  for provider health, shared settings-root health, and local device-head-root
  health. Scheduler/UI callers can render or gate degraded sync status without
  reinterpreting broadwebd's nested fixture health payloads or opening sockets.
- The settings sync runner can now wrap one bounded sync cycle with before and
  after health reports using the same in-process daemon path. This gives the
  eventual scheduler/UI a single result that shows whether a degraded local
  state recovered after publishing or receiving, without starting background
  services or using socket-based fixtures.
- broadwebd provider health now reports concrete fresh, stale, and offline
  provider ids from the in-process fixture, and the scheduler's selected
  retention-provider plan classifies stale/offline handles separately from
  unknown handles. Runtime selected-provider runs reject stale or offline
  selected providers before publishing, pulling, retaining objects, or mutating
  sync roots.
- Scheduler retention-provider selection now also classifies fresh online
  providers that lack the required availability or object-transfer role as
  ineligible rather than unknown. Runtime selected-provider runs reject those
  partial-role providers before publishing, pulling, retaining objects, or
  mutating sync roots.
- Root health now reports delayed mutable-root candidate counts and publisher
  provider ids from the in-process fixture. The profile-sync runner surfaces
  those fields in scheduler-facing health reports, so delayed root propagation
  can be distinguished from a genuinely empty root without loopback sockets or
  external discovery.
- Root health now also reports delayed object-transfer provider ids when the
  latest visible root object is held by a fresh online provider but transfer to
  the requester is paused. The profile-sync runner surfaces that signal so
  scheduler-facing health can distinguish missing objects from local fixture
  transfer delay without opening sockets or contacting external services.
- Runtime health coverage now exercises those delayed root and object-transfer
  signals through arbitrary in-process provider IDs, not only device-shaped
  fixture providers. This keeps future provider-backed sync paths testable
  without modeling every provider as a logged-in Slate device.
- `slate-profile-sync` now has a typed settings sync cycle policy carrying
  retention, publish-step, trusted-device, and retaining-provider quorum
  limits. The runtime-facing policy path checks provider health before touching
  credentials or publishing, so tests can prove an availability-only provider
  is rejected locally instead of failing later as an attempted mutable-root
  write.
- The runtime-facing settings sync runner can now load the active content-key
  id from `slate-settings.db` before running a policy-gated cycle. The caller
  still supplies the actual content key and device signer secret material, so
  the database remains metadata-only while the scheduler no longer has to pass
  duplicated key ids around.
- Settings sync credential preflight now also requires the supplied signer to
  belong to `slate-settings.db`'s local sync device id. A trusted remote device
  public key in the same profile is not enough to publish local device-head
  state, preventing local cycles from accidentally signing with another
  enrolled device's identity.
- The runtime-facing runner now exposes an explicit read-only settings sync
  preflight. It samples provider/root health, applies before-cycle provider
  policy, loads active content-key metadata, validates the local signer
  identity, and enforces the trusted-device bound before any encrypted
  publish/pull attempt.
- Preflight now also includes discovered retention-capable provider records.
  The list is filtered to fresh online providers that advertise both
  availability and object-transfer roles, letting future scheduler/UI code
  choose concrete logged-in devices or availability-only providers instead of
  relying only on aggregate health counters.
- Settings sync cycle policy now supports explicit provider quorum checks for
  fresh online, object-transfer, availability, and mutable-root providers. The
  in-process fixture covers a policy that refuses to run with only one fresh
  provider and then succeeds once a second simulated device provider is present,
  without opening sockets or contacting any external discovery service.
- Settings sync cycle policy can now also cap stale online providers. This lets
  the runtime scheduler reject a provider set that has enough fresh peers to
  meet quorum but still contains stale online peers the selected policy does
  not want to tolerate.
- Settings sync cycle policy can now also cap offline providers. This lets the
  runtime scheduler reject a provider graph where enough fresh providers remain
  to meet quorum but too many known devices or availability providers are
  currently unreachable.
- Policy-gated settings sync cycles now also check root health after the bounded
  publish/pull attempt. Missing roots can still recover during an initial
  healthy-provider cycle, but the runtime path reports a policy error if the
  resulting settings root or local device-head root does not meet the configured
  online retaining-provider quorum.
- Published settings sync cycles now expose the encrypted object ids they
  created. An in-process availability-provider daemon can retain that object
  set through the same fixture network, and the regression proves root quorum
  recovers without opening loopback ports or contacting external protocol
  services.
- The active-key policy runner can now accept availability-provider daemons for
  the same cycle. It publishes/pulls through the local device daemon, asks those
  providers to retain the published object set, reports per-provider retention
  status, and only then enforces the strict post-cycle root quorum.
- The same runtime-facing cycle can now apply shared settings-root candidates
  and retain the union of locally published objects plus received candidate
  objects before checking shared settings-root health. Receive-only candidate
  cycles can keep local device-head health relaxed while still requiring the
  shared settings root to meet the selected retaining-provider quorum.
- `slate-profile-sync` now has an initial runtime scheduler facade. One
  explicit tick accepts a profile/root config, caller-held content key and
  signer secret material, the selected broadwebd daemon, and explicit retention
  provider daemons; it then runs the active-key shared-root candidate cycle,
  hands verified object ids to providers, and returns the same health and
  retention report used by fixture tests. The facade can now also accept
  provider-id/daemon handles and filter them against preflight's discovered
  retention-capable providers, letting fixture scheduler tests model provider
  selection entirely inside `InProcessBroadwebNetwork` without loopback ports or
  external discovery. The same selection logic is available as a read-only
  scheduler plan that reports selected, undiscovered, and duplicate handles
  before any publish, pull, retain, or root mutation occurs.
- Scheduler selected-provider plans and runs now expose structured selection
  issues for stale, offline, ineligible, undiscovered, and duplicate retention
  providers. UI and scheduler callers can render degraded provider selection
  without reinterpreting the raw id buckets, while policy quorum checks remain
  responsible for deciding whether a cycle can proceed.
- Stored-provider scheduler plans and runs now forward that same structured
  selection issue summary through the `slate-settings.db` provider-metadata
  path, including membership-aware, in-process fixture, and protocol
  materializer wrappers. This keeps provider metadata UI/status callers from
  reaching into embedded scheduler plans.
- Stored-provider metadata filtering now also exposes structured metadata
  issues for disabled, locally role-ineligible, and unauthorized providers
  before they enter broadwebd discovery. Scheduler and UI callers can explain
  why a `slate-settings.db` provider never becomes a retention candidate
  without reinterpreting the raw filter buckets.
- The scheduler can now also build a read-only retention-provider plan from
  enabled storage-provider metadata in `slate-settings.db`. Stored providers
  must locally advertise object-transfer and availability before they are
  compared with broadwebd discovery; disabled, locally role-ineligible, stale,
  offline, broadweb-role-ineligible, and undiscovered providers are reported
  separately without publishing, retaining, mutating roots, opening sockets, or
  contacting external services.
- Stored retention-provider metadata can now drive one scheduler tick when the
  runtime supplies matching already-materialized provider daemon handles. The
  run reports selected stored providers that have no materialized handle,
  counts only materialized selected providers toward the retention quorum, and
  performs the publish/retain/root-health sequence entirely through
  `InProcessBroadwebNetwork` fixture daemons in tests.
- Stored retention-provider metadata can now also drive the compaction-only
  scheduler tick. The scheduler loads authorized providers from
  `slate-settings.db`, reports unmaterialized or pending endpoint providers
  before retention, and still hands compaction objects only to daemon handles
  supplied by runtime code.
- Stored-provider compaction can now derive its active content key from
  `SlateSyncSecret` after scheduler preflight. A socketless fixture decrypts
  the published compaction manifest with the derived key, proving callers do
  not need to pass raw content-key bytes for this compaction path.
- Stored-provider compaction can now also run from in-process fixture daemon
  refs. The local-only path materializes stored synthetic fixture endpoints
  into normal retention-provider handles, reports missing fixture providers,
  and compacts through `SlateSyncSecret` without exposing fixture model state
  to scheduler logic.
- Stored-provider compaction can now also run through the socketless protocol
  materializer boundary. The scheduler consumes selected multiaddr/deferred
  endpoint work, accepts materialized provider daemons from the materializer,
  and compacts with `SlateSyncSecret` while preserving real protocol
  request/response boundaries.
- Internal protocol models are constrained to transport/test boundaries:
  protocol implementations must keep real-web request/response semantics and
  can only swap socket IO for internal shims in local deterministic fixtures.
- `InProcessBroadwebNetwork` can now be constructed with explicit profile-sync
  fixture capacity limits for local simulations. The shared local fixture store
  fails closed when simulated providers, object refs, or mutable-root entries
  exceed those caps, keeping multi-device test benches from silently growing
  unbounded state while still avoiding loopback sockets or external networks.
- The profile-sync boundary gate now rejects simulator endpoint/model language
  from production protocol and routing modules. Fixture models can still live
  in the dedicated test-fixture layer, but IPFS, Kubo/IPNS, Tor, protocol
  routing, and gateway code must stay shaped like real network adapters whose
  sockets are replaced only through executor or transport shims.
- Stored-provider runtime ticks now require supplied materialized provider
  handles to match the stored `endpoint_ref` when one is configured. Endpoint
  mismatches are excluded from the materialized provider quorum, preventing a
  provider-id match from silently using the wrong local fixture or future
  protocol endpoint.
- Stored-provider plans now classify endpoint refs before runtime
  materialization. `InProcessBroadwebNetwork` now mints
  `slate-fixture-profile-sync://<network>/<provider>` refs for socketless
  profile-sync fixtures, with the scheme, prefix, and pure parser exported from
  broadwebd so fixture minting, profile-sync classification, and future
  materialization code share one validator. Matching provider-scoped fixture
  refs are marked as in-process endpoints, malformed fixture refs fail closed,
  multiaddr and deferred protocol refs are tracked separately, and
  loopback-shaped `http://` or `https://` refs are unsupported so fixture tests
  cannot silently open sockets or rely on DNS.
- Stored-provider multiaddr endpoint classification now uses the shared
  `slate-routing` `Multiaddr` parser instead of local slash parsing. Malformed
  multiaddr-shaped endpoint refs with empty or invalid segments fail closed,
  while valid multiaddrs remain pending protocol materialization.
- Selected endpoint materialization requests now expose parsed `Multiaddr`
  values for valid multiaddr endpoints. Future protocol materializers can
  consume structured routing targets from the scheduler handoff without
  reparsing provider metadata or opening sockets during planning.
- The selected endpoint materialization plan now also groups valid multiaddr
  endpoints into provider-id plus parsed-`Multiaddr` request records. This
  gives future profile-sync protocol adapters a direct typed work queue while
  missing and deferred-protocol endpoints remain pending materialization.
- Multiaddr provider materialization requests now also expose `RoutingPlan`
  records with an explicit profile-sync privacy boundary. Future protocol
  materializers can consume the same routing-layer type used by broadweb
  navigation without treating stored provider endpoints as normal browser
  visits or falling back to public gateways.
- Deferred protocol endpoints now have the same typed scheduler handoff for
  `provider:` and `iroh-node:` refs: invalid deferred targets fail closed, and
  valid refs are grouped into provider-id, protocol, and target records for
  future protocol materializers.
- Deferred protocol materialization requests now also group into stable
  protocol-keyed batches. Future `provider:` and `iroh-node:` adapters can
  consume only their own selected provider work queue without re-scanning the
  whole scheduler plan or opening any sockets during planning.
- `slate-profile-sync` now has a socketless protocol provider materializer
  boundary. It converts selected multiaddr and deferred-protocol provider work
  into endpoint targets, accepts already-created provider daemons from a
  fixture or future adapter, verifies provider id and endpoint-ref matches, and
  reports missing, mismatched, duplicate, or unsupported providers before those
  handles can feed the stored-provider scheduler path.
- Socketless protocol materializer policies can now cap the number of providers
  materialized in one pass. Otherwise-valid providers beyond that cap are
  reported as capacity-exceeded providers instead of being silently accepted,
  which gives local simulations and future constrained runtime adapters a
  deterministic resource limit without changing scheduler selection semantics.
- Stored-provider scheduler runs can now use that protocol materializer
  boundary directly. A selected stored provider with a multiaddr endpoint can
  be materialized into a normal retention-provider handle, pass the existing
  endpoint-ref checks, and satisfy the retention quorum in the socketless
  in-process broadweb fixture without adding any live network dependency.
- The Iroh-shaped deferred-protocol materializer happy path now runs through
  framed broadwebd clients for both the scheduler and the materialized provider
  handle, keeping the internal model aligned with the future daemon byte
  boundary while still avoiding loopback sockets and real relays.
- Stored-provider scheduler runs now also cover a socketless Kubo
  profile-sync provider materialized from a deferred `provider:` endpoint. The
  regression publishes a `slate-settings.db` snapshot, updates the settings and
  local device-head roots, and retains the resulting objects through
  broadwebd's in-process Kubo fixture without loopback sockets, DNS, public
  IPFS/IPNS, or an external Kubo daemon.
- That Kubo-shaped deferred-provider scheduler path now runs through the
  broadwebd framed-client adapter, so the socketless Kubo/IPNS fixture also
  exercises the future daemon byte boundary.
- The same socketless Kubo scheduler path now covers retained-object
  verification failures. If Kubo `pin/ls` reports a just-pinned manifest as not
  recursively pinned, the run returns structured retention issues and degraded
  root health without requiring a live Kubo node, a loopback listener, DNS, or
  external IPFS/IPNS.
- Membership-log stored-provider scheduler runs now have the same protocol
  materializer path. A provider enrolled through the account membership log and
  stored with a multiaddr endpoint can be materialized through the socketless
  boundary, then retain both settings and membership-log publications without
  opening loopback sockets or contacting live protocol networks.
- Stored-provider plans now also expose read-only protocol materialization
  previews. Scheduler/UI callers can ask a socketless or future real
  materializer which selected providers would become retention handles, compare
  that against the normal stored-provider handle materialization report, and
  prove the preview leaves revisions and profile-sync roots unchanged.
- Membership-log stored-provider plan attempts now expose the same optional
  protocol materialization preview. Credential-blocked attempts still return no
  preview, while ready attempts can report whether selected multiaddr or
  deferred-protocol providers would materialize before publishing settings,
  membership-log objects, or roots.
- The selected endpoint materialization plan now exposes a combined protocol
  materialization summary that groups multiaddr work, deferred-protocol work,
  missing endpoints, and fail-closed endpoints. Scheduler/UI code can tell
  whether a real adapter is required and whether the selected provider set is
  ready before any publish, retain, or root mutation.
- Stored-provider plans now expose that selected protocol materialization
  summary directly, so scheduler/UI callers can inspect protocol work from the
  stored-provider plan without rebuilding endpoint plans themselves.
- Stored-provider runtime result types now expose the same selected protocol
  materialization summary. Successful runs can still report fail-closed or
  pending protocol endpoints that were excluded from quorum, letting callers
  surface follow-up materialization work without inferring it from raw ids.
- Membership-aware stored-provider plan and plan-attempt previews now expose
  selected protocol materialization summaries too. Credential-blocked attempts
  report no protocol summary, while ready attempts expose missing,
  fail-closed, multiaddr, and deferred-protocol work before membership-aware
  runtime mutation.
- In-process fixture stored-provider run wrappers now forward the same selected
  protocol materialization summary as their wrapped runtime result. Local-only
  fixture callers can inspect follow-up protocol work without bypassing the
  fixture-specific result type.
- Protocol-materialized stored-provider run wrappers now expose owned
  materialization reports and count helpers. Scheduler/UI callers can inspect
  materialized, missing, mismatched, duplicate, and unsupported provider
  outcomes after a run without borrowing the live materializer result.
- Protocol-materialized stored-provider previews and read-only plans now expose
  the same protocol blocked/ready helpers. UI or scheduler code can distinguish
  missing/fail-closed endpoint metadata from materializer-blocked provider
  handles before any publish, retain, root mutation, socket dial, or fixture
  daemon handoff.
- Protocol materialization reports now also expose structured materialization
  issues for missing, endpoint-mismatched, duplicate, and unsupported protocol
  providers. Scheduler/UI callers can render socketless or future real-adapter
  failures without reinterpreting raw id buckets or contacting external
  networks.
- Legacy `fixture:<provider>` endpoint refs now also fail closed. They do not
  carry a fixture network id, so they cannot safely satisfy stored-provider
  materialization or quorum checks.
- Stored-provider plans now expose provider-id buckets and counts for each
  endpoint materialization status: in-process fixture, missing, multiaddr,
  deferred protocol, and unsupported. This gives runtime and UI code a
  structured preview before starting or selecting any provider daemon.
- The same endpoint buckets are also available for selected retention
  providers after preflight, so materialization code can focus on the providers
  that are actually eligible for the next scheduler run.
- Selected endpoint buckets now also have a compact materialization preview:
  fixture-ready providers can use the current local-only handles, missing,
  multiaddr, and deferred-protocol providers are pending materialization, and
  unsupported providers fail closed.
- Selected endpoint materialization previews and plans now expose structured
  issues for missing endpoints and unsupported fail-closed endpoints. Multiaddr
  and deferred-protocol endpoints remain protocol work queues rather than
  blocker issues, so callers can distinguish adapter work from invalid provider
  metadata.
- Selected endpoint materialization now also exposes ordered request records
  carrying provider id, endpoint ref, and endpoint status. Future multiaddr or
  protocol-specific materializers can consume the scheduler handoff without
  re-reading `slate-settings.db`, reparsing provider metadata, or opening
  sockets during read-only planning.
- The selected endpoint handoff now also has a socketless materialization plan
  that partitions requests into fixture-ready, pending-protocol, and
  fail-closed groups. Runtime code can decide whether in-process fixtures are
  enough, whether a real protocol materializer is required, or whether the
  selected provider set must fail closed before any adapter dials or binds.
- Stored-provider runtime reports now distinguish fixture-ready providers that
  simply lack a supplied in-process daemon handle from selected endpoints that
  still need protocol materialization, such as missing, multiaddr, or deferred
  protocol endpoints. The materialized quorum count subtracts that pending
  bucket separately from endpoint mismatches and fail-closed endpoints.
- Stored-provider plans now also expose a read-only handle materialization
  report before a runtime cycle starts. Runtime and UI code can compare selected
  providers with supplied handles, see materialized provider ids, pending
  protocol work, endpoint mismatches, and fail-closed endpoints, then decide
  whether it is safe to proceed without publishing or retaining anything.
- Stored-provider handle materialization reports now also expose structured
  materialization issues for unmaterialized, pending-protocol, endpoint
  mismatch, duplicate-handle, and unsupported-endpoint providers. UI and
  scheduler callers can render why quorum failed without reinterpreting raw id
  buckets or opening any fixture, protocol, or socket transport.
- Provider retention runs now expose structured retained-object issues for
  objects that were not retained or not available after a retention attempt.
  Aggregate retention cycle results forward the same issue list, so scheduler
  callers can explain pinning/availability failures without scanning raw
  per-object status booleans.
- Stored-provider handle materialization now rejects duplicate supplied handles
  for the same selected provider instead of choosing the first one. The preview
  and runtime result report ambiguous provider ids separately, and those
  duplicates are excluded from materialized quorum before any sync mutation.
- The stored-provider runtime regression now covers that duplicate-handle path:
  a selected provider with two supplied handles fails the materialized-provider
  quorum check and leaves profile-sync roots unchanged.
- The membership-aware stored-provider runtime now covers the same
  duplicate-handle failure after membership trust has been established: the
  duplicate handles fail quorum and the existing profile-sync roots remain
  unchanged.
- The in-process fixture-daemon stored-provider runtime now also covers
  duplicate daemon refs: duplicate fixture providers fail the materialized
  quorum before publishing pending local settings, and the profile-sync roots
  remain unchanged.
- The membership-aware in-process fixture-daemon stored-provider runtime now
  covers duplicate fixture refs after membership trust is established:
  duplicate fixture daemons fail quorum and existing profile-sync roots stay
  unchanged before pending settings publish.
- Selected synthetic fixture endpoints now expose materialization targets with
  provider id, fixture network id, and endpoint ref so local-only fixtures can
  bridge stored metadata to in-process providers without opening sockets.
- The fixture targets can be validated into scheduler retention-provider
  handles from caller-supplied in-process fixture daemons. Missing, duplicate,
  or wrong-network providers are reported and excluded instead of being used.
- In-process fixture materialization reports now also expose structured issues
  for missing providers, fixture-network mismatches, and duplicate fixture
  daemon refs. Stored-provider fixture-run wrappers forward those summaries, so
  local-only scheduler/UI callers can explain fixture quorum failures without
  opening loopback sockets.
- The stored-provider scheduler can now run directly from those in-process
  fixture daemon refs, preserving socketless tests while exercising the normal
  stored-provider quorum and retention path.
- The membership-log stored-provider scheduler now has the same fixture-daemon
  run path, covering membership log pull/publish behavior and retained settings
  objects through local-only fixtures.
- Stored-provider runtime ticks now also exclude unsupported endpoint refs from
  the materialized retention-provider quorum, even if the caller supplies a
  daemon handle with the same socket-shaped endpoint string. This keeps
  loopback metadata from satisfying socketless fixture runs.
- Stale synthetic fixture refs now fail closed at the same boundary. If a
  stored `slate-fixture-profile-sync://` endpoint names a different provider,
  the selected-provider plan creates no in-process fixture materialization
  target, the protocol materialization plan reports a fail-closed endpoint, and
  supplied fixture daemons cannot publish settings or mutate sync roots.
- The in-process fixture-daemon stored-provider path now covers that
  unsupported-endpoint boundary too: a matching fixture daemon cannot
  materialize a provider whose stored endpoint is loopback-shaped, and strict
  quorum fails before pending settings publish or roots mutate.
- The membership-aware stored-provider scheduler now covers the same
  unsupported-endpoint boundary after membership trust is established: the
  read-only plan marks the loopback-shaped endpoint as fail-closed, strict
  quorum excludes its supplied handle, and pending settings do not publish or
  mutate profile-sync roots.
- The membership-aware in-process fixture-daemon stored-provider path now
  covers that boundary as well: even after membership records have been pulled,
  a matching fixture daemon cannot materialize a loopback-shaped stored
  endpoint, and strict quorum fails before pending settings publish.
- The membership-aware scheduler now has the same stored-provider path. Its
  read-only plan keeps the existing no-mutation boundary and will not pull
  membership records to satisfy credentials, while the runtime path pulls the
  membership log first, then selects enabled retention providers from
  `slate-settings.db`, reports unmaterialized stored providers, and retains the
  resulting membership/settings publication set through supplied in-process
  provider handles.
- The read-only scheduler plan now also proves stale selected retention-provider
  handles are excluded from the fresh candidate set and reported before any
  local revision or sync-root state is mutated.
- Scheduler runs using selected provider handles now reject a selected
  retention-provider set that cannot satisfy the requested retaining-provider
  quorum before publishing local objects, pulling candidates, retaining
  objects, or mutating sync roots.
- Scheduler-facing fixture coverage now includes retention quota and
  pinning-policy failures from a selected availability provider. The cycle
  surfaces the local refusal as a retention error instead of reporting
  successful durability.
- The local profile-sync fixture can now mark simulated devices offline and
  online, allowing tests to verify unavailable devices fail closed without
  touching sockets, DNS, Tor, IPFS, or external relays.
- broadwebd's local profile-sync fixture now registers simulated providers and
  tracks retained encrypted objects per provider. Provider discovery reports
  online in-process providers with their own retained-object counts, so tests
  can model availability loss without implying that one device retaining bytes
  makes every device a pinning provider.
- The fixture object store is now provider-aware: encrypted bytes are available
  only while at least one provider holding them is online, and retaining an
  object copies it into the retaining provider's in-process store. This lets
  tests model simple handoff and availability loss without any sockets.
- The local profile-sync fixture can now delay object transfer between two
  simulated devices. Delayed transfers stay inside the process and make the
  target device treat the source provider's encrypted bytes as unavailable
  until the fixture releases the link.
- The local profile-sync fixture can now block retention for a selected
  provider through a simulated local pinning policy. The provider remains
  online and can still transfer encrypted objects, but new retain requests fail
  locally until the fixture policy is reopened.
- The same fixture can also cap retained object count per provider, letting
  tests model quota exhaustion separately from offline providers, transfer
  failures, and role denial. Releasing a retained object frees capacity inside
  the in-process fixture.
- The fixture can also delay mutable-root propagation between two simulated
  devices. Root delay is independent from object transfer, so tests can model a
  device that can fetch a known encrypted object but has not seen the newest
  root record yet.
- The profile-sync service can list visible competing mutable-root candidates.
  The local fixture keeps one candidate per publishing device, resolves the
  newest visible candidate for the legacy root path, and exposes all candidates
  so future merge tests can model equal-control devices publishing different
  signed roots.
- The local profile-sync fixture now distinguishes logged-in device providers
  from availability-only providers. Availability providers can retain and serve
  encrypted bytes, and discovery reports that they cannot publish mutable roots.
- Provider discovery now exposes typed roles for discovery, local
  connectivity, object transfer, availability, and mutable-root publishing,
  with `can_publish_roots` kept as a compatibility flag derived from the
  mutable-root role.
- The local profile-sync fixture now enforces those provider roles before
  object transfer, retention, provider discovery, root discovery, or
  mutable-root publishing. Objects held by providers without object-transfer
  authority are treated as unavailable by other simulated devices.
- The profile-sync service now validates mutable-root ids and backend object
  ids before fixture lookup, retain, resolve, or publish operations, keeping
  malformed path-like or whitespace-bearing identifiers out of the backend
  model.
- The local profile-sync fixture now reports provider health counts for known,
  online, offline, fresh, stale, object-transfer, availability, and mutable-root
  providers, and marks sync degraded when one required role has no fresh online
  provider. Freshness is modeled with an explicit fixture sequence floor rather
  than wall-clock sleeps, sockets, or background network polling.
- The same fixture can now report health for a concrete mutable root, including
  visible candidate count, latest root object availability, and whether that
  object is retained by enough fresh online providers to satisfy the caller's
  local quorum policy.
- Storage now provides `EncryptedSyncObject` envelopes using ChaCha20-Poly1305
  AEAD through `ring`, and the local two-device fixture moves encrypted setting
  change payloads through broadwebd instead of plaintext JSON.
- Storage now also provides `SignedSyncObject` wrappers using Ed25519 device
  keys, and the local two-device fixture verifies the signed encrypted payload
  against device A's trusted public key before decryption and application.
- Storage now exposes typed helpers for opening signed encrypted settings sync
  objects. Runtime code can verify the device signature, decrypt the envelope,
  require the expected profile, domain, object kind, and content-key id, and
  decode manifests, settings snapshots, or setting-change payloads without
  duplicating fixture-only verifier logic.
- Storage now also has an object-set handoff for fetched settings sync data:
  runtime code can pass the fetched manifest object, optional snapshot object,
  and manifest tail objects through one decode helper, then apply the resulting
  verified object set to `slate-settings.db` through the existing manifest
  validation path.
- Storage now defines a small profile-sync object source trait and pull helper
  that resolves a published root, fetches the manifest, fetches the referenced
  snapshot and tail objects, verifies/decrypts the signed object set, and
  returns the verified settings object set without knowing which broadweb
  backend supplied the bytes. The broadwebd local fixture now exercises this
  source-based pull path.
- `slate-profile-sync` now provides the first reusable runtime bridge from a
  `BroadwebDaemon` to storage's protocol-neutral `ProfileSyncObjectSource`
  trait, plus a small publisher for putting encrypted objects, retaining them,
  publishing mutable roots, and checking retention state. Its local fixture test
  publishes, resolves, lists candidates, fetches encrypted object bytes, and
  releases retention without pulling browser rendering into the sync-only build
  path.
- The same runtime bridge can publish a retained root object with retained
  encrypted dependency objects first, matching the snapshot/tail-then-manifest
  order needed by settings sync while keeping object signing and encryption in
  storage.
- `slate-profile-sync` can now publish actual signed encrypted settings tail
  manifests: it turns local `SyncChangeRecord` values into signed
  `setting-change` objects, retains them, builds the storage-owned manifest,
  signs/encrypts that manifest, publishes it as `settings/latest`, and verifies
  the result through the local broadwebd fixture.
- `slate-settings.db` can now pull and apply a signed settings manifest in one
  storage call once runtime provides an object source and content key bytes. The
  active trusted helper reads the content-key id and profile-scoped device
  public keys from the database, returns `None` for an absent published root,
  applies valid manifests through the existing validation path, and surfaces
  manifest or trust failures without advancing the stored root.
- `slate-settings.db` now has a trusted sync device public-key table and APIs
  to register, update, fetch, list, and locally distrust profile-scoped device
  signing keys. This gives the future runtime sync loop a local trust store
  instead of relying on ad hoc public keys passed in by fixtures.
- `slate-settings.db` can now pull and apply settings sync objects through that
  local trust store: each signed manifest, snapshot, and tail object is parsed,
  matched to a stored trusted device key, signature-verified, decrypted, and
  only then applied. Unknown devices and stale or mismatched stored keys fail
  before the stored root advances.
- Runtime profile-sync preflight ignores distrusted remote device keys, and
  local credential preflight refuses to publish with a local signer whose
  stored key was distrusted. This is only a local revocation primitive; signed
  account membership revocation and enrollment policy are still future work.
- `slate-settings.db` now has a signed account membership record table and
  storage API. Membership records are stored by profile, record id, membership
  epoch, record kind, target device id, signer device id, and exact signed
  bytes; the signed-record helper verifies the signature against the embedded
  signer key and validates enroll, revoke, and device-key-rotation payload
  shapes before accepting them.
- A local membership apply helper can now bootstrap the first self-signed
  device enrollment, then requires later enrollment, revocation, and
  device-key-rotation records to verify against a currently trusted signer key
  before mutating the trusted device-key table. Applied records get an
  `applied_at` marker, so replaying an older enrollment record does not
  re-trust a device after a later revocation.
- Trusted signed settings pulls now also check the stored device-key membership
  epoch against the manifest membership epoch. A key first trusted after the
  manifest epoch cannot authorize that manifest, snapshot, or tail object set.
- `slate-settings.db` now tracks profile-scoped content-key epoch metadata:
  key id, membership epoch, algorithm, active status, and timestamps. The table
  deliberately stores no raw key bytes; secret storage remains keychain,
  recovery-secret, or enrollment work.
- Active-key trusted settings pulls now reject unsupported content-key
  algorithms and content keys introduced after the manifest membership epoch
  before applying or advancing the stored root.
- Active-key trusted settings pulls now have an idempotent root-status helper:
  missing published roots, unchanged already-verified roots, and applied
  manifests are reported distinctly. When the published root matches the stored
  verified root, storage skips object fetch, key lookup, decrypt, and apply
  work.
- Storage now has a typed signed encrypted device-head sync object. Device
  heads identify a profile, device, per-device root, latest manifest object,
  optional latest change object, membership epoch, sequence, and logical clock;
  trusted opening uses the stored device public-key table and rejects device
  keys introduced after the head membership epoch. Unsupported device-head
  schema versions are rejected before runtime merge logic can consume them.
- Storage can now pull a signed encrypted device-head object through the common
  profile-sync object source abstraction. Both explicit-key and trusted-key
  helpers resolve a per-device head root, fetch the object, verify/decrypt it,
  require the decrypted head to name the resolved root, and return the verified
  head with its backend object id.
- `slate-profile-sync` can now publish signed encrypted per-device heads
  through broadwebd. The bridge validates the storage-owned head payload
  against the target root and signer, retains the encrypted `device-head`
  object, and publishes roots such as `settings/devices/<device>/head`.
- `slate-settings.db` can now pull, verify, and record trusted device-head
  roots with the same idempotent root check used by settings manifests:
  missing roots, unchanged roots, and verified updates are reported separately,
  and unchanged roots skip object fetch and decrypt work.
- A verified device head can now drive trusted settings manifest sync directly:
  storage fetches the referenced manifest object id, verifies/decrypts its
  snapshot and tail objects through the trusted-key path, and can apply the
  resulting settings manifest without resolving the global settings root. The
  manifest membership epoch must match the head epoch, and the manifest must
  include a matching frontier for the head device, sequence, and latest change
  object before it can be consumed.
- `slate-profile-sync` can now run the broadwebd-backed trusted device-head
  receive step: resolve and verify a per-device head, verify the referenced
  settings manifest, apply that manifest with the verified head root in one
  `slate-settings.db` transaction when either root is stale, and report
  unchanged only when both roots are already current.
- The local two-device broadwebd fixture now publishes and pulls a trusted
  signed encrypted device head through an in-process per-device root. The test
  retains the head object on the receiving provider before the publishing
  provider goes offline, records the verified head root in `slate-settings.db`
  together with the referenced manifest application, verifies unchanged status
  on the next pull, and follows the head to apply the referenced settings
  manifest while the publisher is offline.
- `slate-settings.db` now has typed snapshot metadata APIs for recording
  encrypted backend object ids, covered revisions, included domains, and latest
  snapshot lookup. This is metadata only; snapshot payloads stay in encrypted
  sync objects.
- `sync_state` now has typed profile sync root APIs so storage can persist the
  last verified manifest object id for roots such as `settings/latest` without
  exposing raw key/value state to callers.
- Storage can now ask a profile-sync object source for visible mutable-root
  candidates, verify each trusted signed settings manifest candidate with
  stored device keys, and apply verified candidates in deterministic
  oldest-to-newest publication order while setting values still use the typed
  logical-clock conflict policy. The broadwebd local fixture bridge covers
  competing `settings/latest` candidates entirely in process. An active-key
  candidate pull path now reports missing, unchanged, and applied states so a
  future runtime poller can skip object fetches when the newest visible
  candidate is already the stored verified root.
- Storage now has a serializable `ProfileSyncManifest` payload with optional
  snapshot object id, tail change object ids, included domains, and device
  frontiers. The local two-device fixture now publishes `settings/latest` to a
  signed encrypted manifest object, then fetches and applies the signed
  encrypted tail setting-change object named by that manifest. Manifests now
  include schema version, membership epoch, and retention-policy metadata so
  local fixtures can model account epoch and future compaction decisions without
  changing the encrypted object shape again.
- Storage now owns a reusable tail-change manifest builder for publish flows.
  It derives tail object order, included domains, created time, and per-device
  frontiers from typed change records plus backend object ids, and the
  broadwebd local fixture uses it instead of hand-assembling simple manifests.
- Storage also owns snapshot-and-tail manifest construction for publish flows,
  deriving compacted frontiers from covered change records and extending them
  with retained tail object ids when a post-snapshot manifest tail remains.
- The `slate-profile-sync` runtime bridge can now publish signed encrypted
  settings snapshot manifests through broadwebd: it retains the snapshot object,
  retains any post-snapshot tail change objects, signs the storage-owned
  manifest, and publishes `settings/latest` to the manifest object id.
- The runtime bridge can now ask `slate-settings.db` for the next settings
  compaction target, derive the covered snapshot domains, publish the signed
  encrypted snapshot manifest through broadwebd, and record the published
  snapshot backend object id plus local settings root in one storage
  transaction so later compaction skips already squashed revisions and local
  verified-root state matches the published manifest.
- Compaction can now run through the same selected-provider retention handoff
  used by regular settings cycles. The runner retains the compacted snapshot,
  any post-snapshot tail objects, and the manifest on selected availability
  providers before strict settings-root quorum is evaluated, with socketless
  fixture coverage in the low-memory boundary gate.
- The scheduler facade can now run that compaction handoff through explicit
  selected provider handles as well, preserving the same stale/offline,
  ineligible, unknown, and duplicate provider reporting used by regular sync
  cycles while keeping the compaction test socketless.
- `slate-profile-sync` now has an initial local publish-flow helper that
  creates a full settings snapshot from `slate-settings.db`, publishes the
  signed encrypted snapshot manifest, publishes the local per-device head
  pointing at that manifest, and records the snapshot metadata plus both
  published roots in one local transaction. This gives new trusted devices a
  complete state handoff without requiring prior snapshot context.
- Incoming synced settings now have an initial deterministic conflict policy:
  the highest logical clock wins, with device id and device sequence as stable
  tie-breakers. Losing setting changes are retained in `settings_changes`
  without updating `settings_values`, the legacy settings view, or watcher
  revisions. The local two-device profile-sync fixture now covers signed
  encrypted replay of a stale setting object against a newer local value.
- Storage now exposes a bounded applied-settings event feed over
  `settings_revisions` and `settings_changes`, giving runtime code a typed
  polling surface for externally synced setting updates without reading raw
  change rows.
- The applied-settings event feed now also has a domain-scoped query. The
  chrome runtime watcher consumes only the `settings` domain, so future app
  watchers can subscribe to Calendar, Contacts, Downloads, and other app
  domains without scanning unrelated replicated payloads.
- Storage now also reports the latest applied revision for a single sync
  domain, allowing future runtime app watchers to initialize and persist
  independent cursors without treating unrelated app-domain updates as their
  own progress.
- Storage now exposes a reusable bounded poll result for one sync domain. It
  returns the previous cursor, latest cursor, and applied event batch so future
  app watchers can share the same local-only polling shape without chrome
  dependencies.
- Chrome now has a runtime synced-settings watcher over that feed. The desktop
  app polls it during the normal Servo pump, applying recognized synced chrome
  zoom and key binding updates through the same in-memory runtime state used by
  `slate://settings` instead of waiting for an internal page request. Its
  startup cursor is initialized from the `settings` domain head, not the
  profile-wide revision, so unrelated app-domain sync does not define chrome
  progress. The watcher now uses storage's persisted raw app-domain cursor, so
  the local runtime cursor can survive restarts and only advances after Chrome
  applies a synced settings batch.
- Storage now has a retention-policy-based settings compaction target helper
  that uses the latest snapshot and applied settings revisions to identify how
  far a future encrypted snapshot can squash while preserving the configured
  unsquashed tail.
- Storage now also has a typed `ProfileSyncSettingsSnapshot` payload builder
  that materializes applied settings values at a target revision with
  deterministic domain and key ordering. The local two-device `profile-sync`
  fixture now seals those payloads into signed encrypted snapshot objects,
  publishes manifests that point at the current snapshot object, verifies the
  snapshot on the receiving device, applies the verified snapshot into
  `slate-settings.db`, records its backend object metadata, and also covers
  replaying retained manifest tail changes after the snapshot has been applied.
- Storage now exposes a reusable verified settings-manifest application helper
  that validates manifest schema, snapshot object id, tail object ids, profile,
  and included domains before applying the current snapshot, replaying the
  manifest tail in order, recording snapshot backend metadata, and advancing the
  stored `settings/latest` root. Those writes now happen in one
  `slate-settings.db` transaction, so a bad tail payload cannot leave a
  snapshot or materialized setting applied without the verified root.
- Storage can now apply a verified settings snapshot by replaying its text
  values through the same conflict policy used for incoming tail changes:
  snapshot values materialize in `settings_values` and the legacy settings view
  when they win, while stale snapshot values are retained without overwriting a
  newer local or incoming winner.
- IPFS/IPNS is the first concrete backend under consideration, but the product
  goal is protocol-neutral: approved Slate devices should find each other, move
  encrypted sync objects, and optionally use approved providers to keep those
  objects available.
- The account model must not depend on a root server, a root node, or one
  always-hot device. Authorized logged-in devices should have equal profile
  control; hot devices and contracted providers only improve encrypted object
  availability.
- The profile-sync bridge can publish and receive signed account membership
  records through broadwebd's `InProcessBroadwebNetwork`, using retained objects
  and explicit `account/membership/<record-id>` roots so authority records move
  between simulated devices without loopback ports or external protocols.
- A discoverable `account/membership/log` index can now be published from
  `slate-settings.db` membership history and received through the same
  in-process fixture transport. It is non-authoritative discovery metadata:
  each listed object is checked against its signed membership payload before
  storage applies enrollment or revocation. Applying a pulled single membership
  record or aggregate membership log now advances its verified root in the same
  SQLite transaction as the trust-state mutation, preventing root/trust drift
  after a failed local write.
- A membership-aware settings runner can now pull and apply the membership log
  before the existing settings sync cycle, so local fixture devices can be
  enrolled and then immediately use trusted device-head settings sync without
  loopback sockets or external protocol services.
- Membership log publications now expose their retained object set, and
  broadwebd fixture providers can retain the log and each referenced signed
  membership record through the same in-process availability layer.
- Membership-aware settings cycles can now retain the combined published
  settings and membership object set through selected in-process availability
  providers.
- Active-key preflight can now pull the membership log before credential
  checks, letting a newly enrolled local fixture device pass scheduler-style
  preflight after its trusted key is learned from the distributed membership
  history.
- The scheduler facade now has an explicit membership-aware selected-provider
  run path, with fixture coverage for applying a remote settings head and
  retaining the combined settings plus membership object set through an
  in-process availability provider. A separate read-only membership-log preview
  can now report missing, unchanged, or available remote log roots by resolving
  and decoding only the aggregate log object, without applying trust records or
  advancing verified roots. Explicit-provider and stored-provider membership
  plans now also have fail-soft attempt surfaces that carry that preview through
  local credential blocks, while still returning real errors for backend,
  storage, or policy failures. Fixture coverage now checks both the
  credential-blocked branch before enrollment is applied and the ready branch
  after the membership log has been applied.
- Membership-log receive fixtures now reject mismatched index entries before
  advancing the stored log root or writing trusted device keys.
- Membership logs now have a fixed record-count cap enforced before entry
  fetch or apply, with socketless fixture coverage for oversized indexes.
- Membership-log publishing now enforces the same cap through a count-only
  plan before loading signed membership record blobs, writing any membership
  record objects, or advancing the log root. Local history that needs
  compaction cannot create a distributed index receivers will reject.
- Membership-log publishing now also has a read-only local plan that reports
  empty, publishable, or too-large history from a count-only
  `slate-settings.db` query before any broadwebd call or root mutation. The
  scheduler facade can now include that local membership-log publication
  preview alongside the existing selected-provider plan, without pulling remote
  membership or advancing roots.
- Membership-aware runner and scheduler runs now reject oversized local
  membership history from that preview before pulling remote membership,
  publishing settings objects, or advancing any profile-sync roots.
- `slate-settings.db` now enforces the first membership epoch-ordering rule:
  after a newer membership operation for a target device has applied, a
  different older-epoch operation for that device is rejected and not stored.
  A different same-epoch operation for a target device is also rejected once
  another record at that epoch has applied, while exact replay of an
  already-applied record remains idempotent.
- Membership authorization now also checks the signer's own trust epoch before
  applying a signed membership record. A device key first trusted in a later
  epoch cannot authorize an older account membership operation, preventing newly
  enrolled devices from rewriting earlier account history.
- Device-key rotation records now require the target device to already have a
  trusted key. A rotation cannot enroll a new device or re-trust a revoked
  device; revoked devices must use an explicit later `enroll-device` record.
- Enrollment records now reject already trusted target devices. This keeps
  `enroll-device` for new or explicitly revoked devices and requires
  `rotate-device-key` for trusted-device key replacement.
- The broadwebd membership-log receive fixture now covers that epoch-ordering
  rule over in-process object transfer: a stale older-epoch record is rejected
  without storing it, re-trusting the device, or advancing the membership-log
  root.
- The same socketless membership-log receive path now covers duplicate
  enrollment rejection: a later `enroll-device` record for an already trusted
  target device fails without replacing that device key or advancing the
  membership-log root.
- Membership-log application is now transactional inside `slate-settings.db`.
  The receiver fetches and validates every signed record object first, then
  applies the whole log in one storage transaction; if any record fails, earlier
  records in that log do not leave partial trusted-key mutations behind.
- Applying signed membership records now also materializes the target device in
  `sync_devices` with the applied membership epoch and no provider authority by
  default. That device-roster write is part of the same membership transaction,
  so invalid roots or failed records cannot leave orphaned device metadata.
- Runtime trusted-device selection now consults `sync_devices` and excludes
  devices explicitly marked with provider authority from settings-head pulls.
  This keeps future provider records from gaining profile write authority merely
  because they have a trusted key record.
- Direct trusted device-key registration now also materializes the matching
  `sync_devices` roster row transactionally. Updates preserve existing labels
  and provider-authority metadata while advancing the membership epoch.
- Account membership authorization now also rejects trusted signers that are
  explicitly marked with provider authority in `sync_devices`. Provider-backed
  availability records can hold trusted keys for object retention without being
  able to enroll, revoke, or rotate profile devices.
- Trusted profile-state opens now also reject provider-authority signers before
  accepting device heads, manifests, snapshots, or tail changes. This closes the
  direct storage path so availability providers can retain encrypted bytes
  without being able to publish or apply signed settings state.
- The socketless broadwebd device-head receive fixture now covers that same
  rule at the runtime boundary: a provider-authority signer can publish a head
  object into the simulated network, but the receiver rejects it without
  advancing the stored per-device root.
- The shared-root receive path now applies the same provider-authority
  boundary through the socketless broadwebd transport: a provider-authority key
  can publish a syntactically valid signed encrypted `settings/latest`
  candidate, but the receiver rejects it before mutation and keeps the last
  trusted materialized setting/root.
- The account membership log now has an `enroll-provider` record kind for
  account-authorized availability providers. Applying the record stores the
  provider public key and materializes `sync_devices.provider_authority = true`
  transactionally, so providers can be distributed through the membership log
  without gaining membership-signing or profile-state authority.
- The socketless broadwebd membership-log bridge now covers provider enrollment
  end to end: a publisher emits `enroll-provider` through the aggregate log, the
  receiver validates and applies the signed provider record, records the
  provider-authority roster row, and still excludes that provider from trusted
  settings-head pulls.
- Stored retention-provider selection now requires the stored provider id to
  also be a trusted provider-authority device in `slate-settings.db`. The
  scheduler reports unauthorized stored provider metadata separately and
  excludes it before endpoint materialization, so synced provider listings
  cannot satisfy retention quorum unless the account membership log authorized
  that provider.
- The Settings profile-sync preview now exposes provider-authority readiness as
  trusted/total counts from `slate-settings.db`, so local trials can distinguish
  a configured storage provider from one that the replicated membership log has
  actually authorized for retention.
- The preview also exposes the concrete authorized retention provider ids from
  local readiness JSON and renders matching provider names separately from
  merely active/configured storage providers. This keeps local trials honest
  about which providers can satisfy retention quorum under the membership log.
- The same membership-log bridge now covers the provider-signer failure mode
  transactionally: if a log enrolls a provider and then uses that provider to
  sign an account membership change, the whole pulled log is rejected, the
  provider key is not materialized, and the membership-log root is not advanced.
- Aggregate membership-log validation now fail-closes on unsupported record
  kinds before preview or pull paths fetch individual signed records. The
  accepted set is limited to device enrollment, provider enrollment, revocation,
  and device-key rotation.
- Aggregate membership-log validation now also checks record, target-device,
  and signer-device identifiers before object fetch. Malformed ids fail during
  socketless preview without advancing the local membership-log root.
- `slate-settings.db` now has the first local device-enrollment bundle for
  future QR-code or file-based onboarding. The bundle carries an ordered,
  bounded chain of signed membership records, validates profile and target
  device metadata, requires an enrollment or key-rotation record for the target
  device, and imports only when the target matches the local sync device id.
  Import still uses the same transactional signed-membership application path,
  so the artifact adds no root server, no plaintext sync payloads, and no
  socket or live protocol dependency.
- Storage now has the first in-memory `SlateSyncSecret` helper for key
  separation. It derives profile-scoped content encryption keys with HKDF over
  the profile id and content-key id, redacts the root bytes from debug output,
  and is covered by a local sync-object encryption regression. This does not yet
  define the persisted/exported recovery-secret format or mutable-root
  delegation keys.
- The broadwebd settings-sync runner can now use that `SlateSyncSecret` after
  active-key preflight: it loads the active content-key id from
  `slate-settings.db`, derives the content key in memory, publishes through the
  normal encrypted settings cycle, and a second socketless fixture device can
  derive the same key and apply the signed encrypted device head. Plaintext
  keys still stay out of `slate-settings.db`.
- The scheduler facade also has a secret-backed retained tick. It performs
  active-key preflight, derives the content key from `SlateSyncSecret`, runs the
  same shared-root candidate cycle, and hands the published object set to a
  selected local-only retention provider without requiring the caller to handle
  raw content-key bytes.
- Membership-aware settings sync can now use `SlateSyncSecret` too. The runner
  pulls and applies the account membership log before credential validation,
  loads the active content-key id from `slate-settings.db`, derives the content
  key in memory, and then applies signed encrypted settings from a newly trusted
  fixture device without loopback sockets or raw content-key storage.
- The membership-aware selected-provider scheduler path now has the same
  secret-backed form. After membership-log preflight enrolls the local device,
  the scheduler derives the active content key from `SlateSyncSecret`, applies
  the trusted remote device head, and retains the published membership/settings
  object set through a selected in-process provider.
- Stored-provider scheduler runs can now derive the active content key from
  `SlateSyncSecret` too. A local-only fixture stores one authorized provider in
  `slate-settings.db`, materializes the matching endpoint handle, publishes
  encrypted settings, and retains the object set without exposing raw
  content-key bytes to scheduler callers.
- Membership-aware stored-provider scheduler runs now have the same
  secret-backed form. A socketless fixture pulls membership records for a newly
  enrolled local device and provider, derives the active key from
  `SlateSyncSecret`, applies encrypted settings, and retains the combined
  membership/settings object set through provider metadata from
  `slate-settings.db`.
- Protocol-materialized stored-provider scheduler runs can now use
  `SlateSyncSecret` as well. The socketless multiaddr fixture stores provider
  metadata in `slate-settings.db`, materializes the selected provider through a
  caller-supplied protocol materializer, publishes encrypted settings, and
  retains the object set without scheduler callers passing raw content-key
  bytes.
- Secret-backed protocol-materialized compaction now runs through framed
  broadwebd clients for the scheduler, materialized provider handle, and
  post-run manifest fetch, so snapshot compaction also crosses the socketless
  byte boundary before the real daemon IPC transport exists.
- Membership-aware protocol-materialized stored-provider scheduler runs now
  have the same secret-backed path. A local-only fixture publishes the
  membership log, materializes the selected multiaddr provider, publishes
  encrypted settings, and retains the membership/settings object set without
  passing raw content-key bytes into scheduler calls.
- In-process fixture-daemon stored-provider scheduler runs can now use
  `SlateSyncSecret` too. A socketless fixture stores the synthetic fixture
  endpoint in `slate-settings.db`, materializes it through the in-process
  network id, publishes encrypted settings, and retains the object set without
  exposing raw content-key bytes to fixture scheduler callers.
- Membership-aware fixture-daemon stored-provider scheduler runs now have the
  same secret-backed path. A local-only fixture publishes membership/settings
  state through a stored synthetic fixture endpoint and retains the combined
  object set without passing raw content-key bytes into scheduler calls.
- `SlateSyncSecret` now has a domain-separated in-memory hierarchy for
  profile-scoped recovery, manifest-signing delegation, mutable-root
  publishing, enrollment, device bootstrap, and content-key epoch material.
  The derived material is purpose-typed and debug-redacted; user-facing QR/file
  transport and runtime backend policy are still future work.
- `SlateSyncSecret` can now derive stable profile device signers for signed
  profile-state objects from the same manifest-signing material, without
  storing signer private keys in `slate-settings.db`. This gives the upcoming
  encrypted handoff-file and local-only provider paths a persistent signing
  identity tied to the key file instead of temporary generated signers.
- `SlateSyncSecretExport` now provides the first local JSON envelope for future
  QR-code or file-based device login. It carries profile id, schema version,
  URL-safe base64 root-secret bytes, and creation time; import validates the
  schema, exact secret length, and optional expected profile id, while debug
  output redacts the encoded secret.
- `slate://settings` now includes a first session-only Profile Sync Preview.
  It can create a local profile from a Slate Sync Secret and can import the
  same secret-bearing material only through the enrollment-file handoff flow.
  The rendered page no longer exposes the raw key-file download/import controls;
  the root secret remains session/file material rather than plaintext database
  state. This is deliberately local and rough: it does not render QR codes yet
  or contact broadweb providers.
- Creating a local Profile Sync Preview profile or importing an enrollment file
  now activates local non-secret sync metadata in `slate-settings.db`: the local
  device record, default app sync domains, the active content-key epoch id, and
  trusted public membership records derived from the secret material.
- The first key-file enrollment shape now derives a stable account-authority
  signer and a stable local-device signer from `SlateSyncSecret`. Storage
  self-enrolls the account authority, uses it to enroll the local device, and
  treats local readiness as blocked until the local device's derived public key
  is trusted. This is a practical bootstrap for the local preview, not the final
  multi-approval device-governance policy.
- `slate://settings` now has a manual Profile Sync Preview preflight. It reads
  only local `slate-settings.db` metadata and reports whether the local device,
  trusted derived signing key, active content-key epoch, enabled app domains,
  and authorized retention-capable storage providers are present before any real
  publish/pull operation runs. The preview can also seed a local test-provider
  metadata record using a Slate-only fixture endpoint ref, which advances the
  trial without opening sockets, binding loopback ports, or contacting external
  broadweb services.
- The Profile Sync Preview can now run a first local-only trial cycle from
  `slate://settings`. The action reuses the active session key-file secret,
  signs the local preview provider into membership as a retention-only
  authority, writes a harmless preview setting delta, materializes a socketless
  broadwebd fixture provider, publishes encrypted settings and membership
  objects, retains them on the simulated provider, and reports the object
  counts back to the page.
- The Profile Sync Preview can also run a two-device local simulation. The
  preview creates a temporary receiver `slate-settings.db`, derives a
  `ProfileSyncEnrollmentBundle` from the same session key-file secret, applies
  that bundle to the receiver, signs the receiver into the publisher's
  membership log, publishes one encrypted setting from the real local database,
  pulls it into the receiver through socketless broadwebd fixture daemons, and
  reports the receiver-applied setting count. This models file/QR-style
  enrollment and remote application without opening ports or using a public
  protocol.
- `slate://settings` now exposes the first manual enrollment-file preview. A
  session key file can derive a non-secret `ProfileSyncEnrollmentBundle` for a
  target device id, show the JSON for paste/debugging, download it as a small
  file, and import a selected or pasted bundle through the same local
  transactional membership path. This is enough to exercise the expected
  onboarding shape.
- Runtime profile opens now create or reuse a non-secret
  `slate-local-device-id` sidecar next to the selected `slate-settings.db`,
  giving each install a durable sync device id without storing local identity
  inside the syncable database. Fixture-only `open_resolved` paths still use
  the deterministic `local-device` id so local tests remain reproducible.
- The enrollment preview now has a first non-secret device-request file. A new
  device can export its profile id and durable device id, an existing enrolled
  device can import that request to fill the enrollment target, and the
  existing enrollment-bundle flow can then grant membership to the requested
  device. The request carries no root secret, signed membership records, or
  sync payloads.
- `slate-settings.db` now has a first secret-bearing handoff bundle primitive
  for local QR/file experiments. It combines a `SlateSyncSecretExport` with the
  target device's non-secret enrollment bundle, validates nested profile and
  target consistency, rejects wrong-device imports before mutation, applies
  membership, and activates key-derived local sync metadata without persisting
  the raw root secret in the database. The file itself is sensitive login
  material and still needs encrypted recovery/handoff policy before production
  use.
- `slate://settings` now exposes that handoff primitive in the Profile Sync
  Preview as the single visible PC-to-PC enrollment-file flow. An enrolled
  session can download a target-specific
  `slate-profile-enrollment-<device>.json` file, and a target device can import
  a selected or pasted enrollment file to apply membership and activate the
  session key-derived local metadata in one step. The lower-level key-file,
  device-request, and non-secret enrollment-bundle storage primitives remain for
  deterministic fixture logic, but their `slate://settings` protocol routes and
  rendered controls have been removed until QR-code or unbound onboarding
  semantics are designed deliberately. This remains a sensitive local preview
  flow and still uses the existing query-string internal action transport until
  POST/body-capable Slate protocol imports are added.
- The enrollment-file handoff path now has an explicit bounded-import guard
  while it remains on that query-string transport. The Settings page rejects
  oversized selected or pasted enrollment files before building the internal
  action URL, and the `slate://settings` handoff import route rejects oversized
  payloads before parsing secret-bearing JSON. The same storage-owned size
  limit is enforced by the `ProfileSyncSecretHandoffBundle` parser and encoder
  so future non-UI callers do not deserialize or generate unbounded
  secret-bearing handoff files.
- The Settings Profile Sync Preview JSON now follows the same narrower surface:
  normal state responses no longer include raw key-file exports, device-request
  files, or non-secret enrollment-bundle JSON. The secret-bearing
  `handoff_export_text` field is returned only by the explicit handoff-create
  action that backs the enrollment-file download, and editing the target device
  clears any pending generated file text in the page.
- The socketless two-device Profile Sync Preview now uses the same request
  shape: the receiver emits a `ProfileSyncDeviceEnrollmentRequest`, the
  publisher derives the enrollment bundle from that request, and the normal
  signed membership import path applies the result before encrypted settings
  sync runs. This keeps the local-only broadweb fixture aligned with the
  user-facing handoff flow.
- `slate://settings` now renders a compact Profile Sync Preview checkpoint
  panel from the same local JSON state. The manual trial surface shows the
  local device, key state, provider readiness, trusted-device count, app-domain
  count, enrollment-file target, and last local/two-device trial result without
  adding any protocol endpoints.
- The same preview now separates enabled app domains from enabled content-sync
  domains. This makes the current local trial boundary visible: Settings,
  Bookmarks, and Downloads metadata may be enabled while file/content payload
  domains remain disabled until their privacy and retention policies are ready.
- The Profile Sync Preview now also exposes per-domain applied revision heads
  from `slate-settings.db`, letting local fixture trials show whether Settings,
  Bookmarks, Downloads, or future rail-app domains have local sync activity
  without inspecting the database manually.
- The Profile Sync Preview now also has a local-only "Sync current settings"
  action. It reuses the session key file and stored preview provider metadata
  to publish/receive the current pending `slate-settings.db` changes through
  socketless fixture daemons without writing the synthetic preview setting used
  by the older smoke-test trial.
  Production multi-device use still needs QR rendering, encrypted
  handoff/recovery files, real provider daemons, conflict handling, and cadence
  policy.
- The profile-sync scheduler retention-provider boundary now takes
  `BroadwebdClient` trait objects instead of concrete `BroadwebDaemon`
  references. Fixture and protocol materializers can still hand over in-process
  daemons during tests, but the scheduler path no longer assumes Slate is linked
  directly to an in-process daemon when retaining published objects.
- The Settings Profile Sync Preview now separates sync health from issue
  details. After the current-settings or local trial action, the page reports
  whether the latest local fixture run is healthy, still degraded, or recovered
  from a degraded pre-run state, while issue details remain the protocol-neutral
  retained-object, provider-selection, and endpoint problems.
- The Profile Sync Preview health and issue summary now also includes the
  two-device local trial. The publisher-side provider materialization and
  retention issues feed the same protocol-neutral issue rows, while receiver
  before/after health determines whether the PC-to-PC enrollment simulation is
  healthy, degraded, or recovered.
- `make profile-sync-boundary-check` now provides a low-memory regression gate
  for the sync work. It runs focused rail-app sync-domain checks, verifies that
  visible rail app sync descriptors match the seeded `slate-settings.db`
  storage domain table, checks Slate Sync Secret domain separation, export
  round-trip behavior, stable signer derivation, profile-bound import,
  secret-backed enrollment bundle derivation,
  secret-backed local signer enrollment, secret handoff bundle import guards,
  and non-secret local activation metadata, then covers local readiness reports,
  preview provider activation, storage/provider metadata, raw and typed
  app-domain cursors, the broadwebd app-domain fixture, the profile-sync
  scheduler fixture, the Iroh-shaped socketless materializer fixtures, and both
  local Profile Sync Preview trials through the build-limits wrapper. Because
  compiling `slate-chrome` currently pulls Servo
  script bindings and exceeded the 2 GiB low-memory profile during
  verification, the gate also performs static chrome/resource assertions for
  the Settings handoff routes and controls. The static chrome assertions now
  also cover the synced-settings watcher wiring: the desktop app must own and
  poll `SyncedChromeSettingsWatcher`, the watcher must use storage's
  `AppSyncDomainWatcher` for only the `settings` sync domain, and batches must
  flow through the runtime zoom and keybinding apply paths before the watcher
  cursor advances. Full dynamic chrome watcher coverage remains opt-in.
- The next networking checkpoint is deliberately internal-first. Profile sync
  should continue using socketless, deterministic simulated broadweb fixtures
  until the local model covers delayed roots, unavailable objects, stale/replayed
  manifests, conflicting equal-authority device heads, corrupt objects,
  unauthorized membership/provider records, retention gaps, and fail-closed
  endpoint materialization. Online protocol testing comes after those cases are
  reproducible locally.
- The fixture model should explicitly cover IPFS/IPNS, Iroh-like rendezvous and
  transfer, Tor/I2P-style private routing, LAN discovery, and contracted or
  self-hosted retention providers as internal deterministic adapters first.
  Later opt-in real-network probes should compare observed behavior against
  those models and drive fixture refinements when the real web behaves
  differently.
- The first real-network smoke is now available as an opt-in LAN probe rather
  than a committed test. It should remain host-agnostic, memory-limited, and
  cleanup-oriented while the default regression suite continues to use
  socketless deterministic fixtures.
- The first peer-discovery LAN smoke is also available as an opt-in probe. It
  currently uses unauthenticated UDP multicast advertisements, so the next
  production step is to bind advertisements to enrolled device identity,
  membership epoch, and signed provider records before any automatic sync policy
  trusts discovered peers. libp2p, IPNS, Iroh, or mDNS adapters should implement
  the same advertisement semantics rather than bypassing the profile-sync
  scheduler/provider boundary.
- Profile-sync discovery now has a protocol-neutral provider boundary in
  broadwebd. The in-process broadweb fixture can publish and discover
  libp2p-rendezvous and IPNS-shaped multiaddr advertisements without opening
  sockets, resolving DNS, starting loopback services, or joining external p2p
  networks. This is the fast local model for future broadweb discovery tests:
  real libp2p Kademlia/rendezvous, IPNS/IPFS, Iroh, mDNS, relay, or delegated
  routing adapters should plug into the same `ProfileSyncPeerDiscoveryProvider`
  trait instead of adding profile-sync-specific socket discovery paths.
- The first concrete discovery adapter is now Kubo/IPNS-shaped:
  `IpnsProfileSyncPeerDiscoveryProvider` publishes a bounded peer
  advertisement record through Kubo `add`, `pin/add`, and `name/publish`, then
  discovers by resolving configured IPNS names and loading the advertised object
  back through Kubo `cat`. The regression runs this against the socketless
  in-process Kubo model, so local CI validates the protocol request sequence
  without joining the public IPFS network.
- Profile-sync now owns the first trust gate above broadwebd discovery:
  `filter_trusted_profile_sync_peer_discovery_results` accepts only discovered
  candidates whose advertised network matches the selected sync network, whose
  node id is not the local device, whose service-frame capability is present,
  and whose device public key is already registered and trusted in
  `slate-settings.db`. The focused regression covers trusted, revoked, unknown,
  wrong-network, local-device, and wrong-capability advertisements without
  opening sockets or contacting external discovery.
- Discovery advertisements now carry an optional signed identity envelope.
  broadwebd keeps the envelope protocol-neutral and bounded while
  `slate-profile-sync` signs the advertisement body with the local device
  signer and rejects automatic trust for unsigned advertisements, tampered
  advertisement bodies, or same-device advertisements signed by an unenrolled
  replacement key. The focused regressions keep this local and socketless.
- The settings sync scheduler can now take a trusted signed discovery report
  and select retention provider handles only for trusted discovered provider
  ids. Rejected, unknown, unsigned, or tampered discovery candidates never get
  passed into the existing selected-provider policy path. The in-process
  regression keeps the whole path local: two fixture providers exist, only one
  signed/trusted advertisement is accepted, and only that provider retains the
  published settings objects.
- `slate-profile-sync` now owns a discovery execution helper over broadwebd's
  `ProfileSyncPeerDiscoveryProvider` trait. It asks the provider for protocol
  candidates, then immediately applies the signed local trust filter before
  returning a report to schedulers or UI code. The simulated Iroh-rendezvous
  regression publishes signed trusted and unknown advertisements through the
  socketless fixture provider and proves the unknown signer is rejected before
  provider selection.
- The socketless IPNS/Kubo discovery fixture now carries the same signed
  identity envelope through the production-shaped `add`, `pin/add`,
  `name/publish`, `name/resolve`, and `cat` request sequence. This keeps the
  local IPNS model aligned with the signed-discovery trust boundary without
  contacting public IPFS/IPNS, opening loopback services, or changing the
  production Kubo request builder.
- Discovery advertisements now include a signed membership epoch. The
  profile-sync trust filter rejects a discovered peer when the locally trusted
  device key was introduced after the advertisement's claimed epoch, so local
  fixtures can model stale or pre-authorization discovery data before any
  automatic provider selection trusts it.
- The `slate-profile-sync` crate is no longer a single source file. The first
  low-risk split extracts discovery trust filtering, shared object-id
  de-duplication helpers, root-id naming helpers, scheduler-facing health
  reports, membership-log plan/preview DTOs, and error types into dedicated
  modules while keeping the public crate re-exports stable. Each extracted
  module owns focused unit tests so future scheduler/protocol work can change
  those pieces without relying only on the large integration-style test module.

Next:

- Continue extending the protocol-neutral `profile-sync` application service
  toward real protocol-backed provider materialization.
- Wire the trusted discovery execution helper into the runtime scheduler/UI
  path that actively queries configured IPNS, libp2p, Iroh, mDNS, or LAN
  discovery providers before a sync run. The signed trust report and
  scheduler-side provider filtering are now covered locally, but automatic
  discovery execution still needs a runtime orchestration path.
- Add a libp2p rendezvous/Kademlia discovery adapter behind
  `ProfileSyncPeerDiscoveryProvider` if its dependency footprint remains
  acceptable under Slate's low-memory development profile.
- Update the opt-in LAN discovery smoke to produce and consume signed peer
  advertisements with membership epochs before treating it as representative of
  automatic enrolled-account discovery.
- Continue splitting sync backends into discovery, connectivity, transfer,
  availability, and mutable-root implementations so different broadweb
  protocols can be combined behind the typed provider role records.
- Extend the stored signed membership record history into the full
  equal-control account authority model: add epoch transition rules,
  multi-approval policy, recovery-file enrollment, and provider records with no
  profile write authority.
- Implement the actual runtime sync loop that supplies a broadwebd-backed object
  source, trusted account keys loaded from `slate-settings.db`, and explicit
  sync policy checks to the storage pull-and-apply helper.
- Extend the runtime watcher beyond the first chrome settings path so
  externally synced routing, privacy, app, and browser-core changes are applied
  through their normal update paths instead of raw database replacement.
- Extend the Settings-based `Profile Sync Preview` with QR-code rendering for
  smartphone-mediated enrollment, an explicit decision on target-bound versus
  unbound enrollment files, platform key-store loading, device identity
  rotation/repair, POST/body-capable custom protocol imports for larger artifacts,
  trusted-device/provider status, encrypted recovery/handoff-file flows, richer
  multi-device local trial execution, and app-domain status for Settings and
  Bookmarks before exposing real IPFS/IPNS or internet-backed providers.
- Add runtime policy for when derived manifest, mutable-root, enrollment,
  bootstrap, and content-key material can be used.
- Publish encrypted profile manifests and snapshots through the selected
  backend, then point the profile mutable root at the newest manifest.
- Use the storage compaction target to publish encrypted snapshots, trim
  manifest tails, and then enforce retention across broadwebd providers.
- Extend the initial runtime scheduler facade with real provider selection,
  platform key-store/enrollment secret loading, cadence control, visible
  degraded-health handling, and UI status reporting.
- Add device enrollment, revocation, and key rotation before syncing sensitive
  profile domains.
- Expand local distributed-protocol fixtures to simulate offline devices,
  delayed sync, availability loss, pinning policy, and conflicts entirely
  through in-process fixture transports, without loopback ports, the real
  internet, Tor, public IPFS/IPNS, or external relays.
- Continue extending bounded fixture-state accounting from the shared local
  profile-sync fixture store into future protocol models at the socket/transport
  shim layer, not inside production protocol request builders.
- Keep fixture behavior behind transport executors or shims. Protocol adapters
  should still build and parse production-shaped HTTP, Kubo, IPNS, Iroh-like,
  Tor/I2P, LAN, or retention-provider messages; the test layer only swaps the
  socket path for deterministic in-process delivery.
- The Kubo fixture state model must remain private to the fixture layer.
  `kubo.rs` may plan production Kubo requests and select a transport executor,
  but direct fixture model or registry access from the protocol implementation
  is a boundary violation covered by `profile-sync-boundary-check`.
- Commit each coherent step separately and rerun focused regression tests for
  storage, broadwebd, rail apps, and chrome behavior as those areas are touched.

Backlog:

- Add manual real-network probes after deterministic models are stable, then
  compare observed IPFS/IPNS, Iroh-like, Tor/I2P, LAN, and retention-provider
  behavior against the internal fixtures and refine the models where reality
  differs.
- Add ignored/manual loopback Kubo integration tests for add, pin, publish, and
  resolve.
- Add leak tests proving profile sync does not use OS DNS, public gateways, or
  non-local RPC endpoints by default.
- Add UI surfaces for sync health, pinning status, degraded availability, device
  enrollment, and remote pinning policy.
- Keep other transports possible behind the same `profile-sync` service
  boundary once IPFS/IPNS works.
- Add policy surfaces that show when discovery, relays, public gateways,
  contracted pinning, Tor, or other broadweb providers may learn device
  identifiers, timing, object sizes, or traffic volume.
- Add account governance policy for QR enrollment, recovery files, device
  revocation, and future M-of-N approval for high-risk account changes.
- Add typed sync records, merge behavior, and privacy notes for each first-party
  app domain. Files, Contacts, Calendar, Chat, Downloads, Storage, and future
  rail apps should each define whether they sync content bytes, metadata, or
  both before moving beyond generic domain registration.

Future protocol candidates:

- Iroh / `iroh-blobs`: Rust-native verified blob transfer with BLAKE3
  content-addressing. Consider it as the main alternate backend for private
  device-to-device object transfer.
- Syncthing BEP: mature folder sync protocol with device IDs, TLS-authenticated
  peers, block indexes, discovery, and relay support. Consider it as a reference
  design or optional external provider for user file/folder sync.
- Hypercore / Hyperdrive: append-only signed logs and live replication. Consider
  it for device-head logs or append-only profile change streams if the Rust
  integration story becomes practical.
- Tahoe-LAFS: encrypted capability-based distributed storage with erasure-coded
  shares. Consider it later for Storage, backup, or durability-oriented sync
  rather than the first lightweight settings sync path.
- Raw libp2p service: custom Slate protocol over libp2p transports, discovery,
  relay, and request/response streams. Consider only after the sync semantics
  are stable enough to justify owning more protocol surface.

Priority order:

1. Local syncable `slate-settings.db` state: snapshots, sync object metadata,
   conflict policy, and compaction on top of the implemented typed changes,
   revisions, device state, app domains, and local-only application of updates.
2. Runtime settings watcher that applies local typed changes through normal
   browser-core/chrome/routing paths.
3. Broaden the implemented broadwebd `profile-sync` service contract with
   policy checks, richer fixture controls, and backend role separation.
4. Backend role model for discovery, connectivity, transfer, availability, and
   mutable roots.
5. Equal-control account authority model with signed device heads and membership
   epochs.
6. Manifest/snapshot/change encoding, account membership signatures, and
   key-epoch integration on top of the implemented encrypted object envelope and
   device-signature wrapper.
7. Kubo RPC add/pin/IPNS implementation with loopback-only validation.
8. Two-device local sync flow and retention compaction.

## Protocol Testing

- Add `.onion` normalization tests for direct address-bar input, explicit
  `http://` and `https://` onion URLs, and download URLs. Normalized routes
  should become `tor+http://` or `tor+https://` before Servo can attempt normal
  DNS resolution.
- Add broadwebd Tor tests around adapter selection, route metadata, error
  reporting, disabled/unavailable Tor behavior, and DNS-leak prevention. These
  should use mock transports by default.
- Add ignored/manual external tests for Arti-backed Tor retrieval so real
  network behavior can be checked without making the normal test suite depend
  on Tor bootstrap, live onion services, or internet availability.
- Add tests for `.onion` subresources and downloads once the corresponding
  browser paths are implemented.

## Interaction Testing

- Add text-selection tests for injected page-selection script registration,
  drag selection, selected-text context menu behavior, Copy, Select All, and
  Clear Selection.
- Verify editing controls are not regressed by page-selection handling. Native
  text inputs should keep normal copy, paste, cut, select-all, and context-menu
  behavior.
- Add zoom-aware pointer tests so chrome zoom does not offset page hit testing,
  hover detection, link activation, or text selection.
- Add shortcut tests for configurable copy, cut, paste, select-all, new tab,
  close tab, next tab, and previous tab.
