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

_Current focus: safe broadweb sync between Slate devices logged into the same
account._

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
- Storage provider metadata now has a local-first `slate-settings.db`
  materialized table and sync-domain JSON projection with tombstones. The
  initial projection tracks provider kind, display name, endpoint reference,
  broadweb role flags, quota hints, retained-object limits, pinning policy, and
  enabled state. Provider credentials, private keys, local daemon paths, live
  health, and per-device availability stay local to runtime or secret storage.
- The local broadwebd profile-sync fixture now carries typed app-domain
  metadata through the encrypted manifest path between two local
  `slate-settings.db` instances. The regression covers Chat, Files, and Storage
  provider projections over the in-process fixture network, with encrypted tail
  objects and no loopback sockets.
- The `slate-profile-sync` runtime bridge now also has full-snapshot
  publisher/receiver regressions for typed Chat, Files, and Storage provider
  metadata. The tests enable those opt-in domains on the publisher, publish
  signed encrypted device-head snapshots through broadwebd's in-process
  fixture, verify the receiver materializes typed rows, verify post-snapshot
  update tails for all three domains, and verify tombstone snapshots and
  post-snapshot tombstone tails delete stale typed rows on the receiver.
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
- broadwebd has a protocol-neutral `profile-sync` application service with an
  in-memory local fixture backend. Unit tests cover object transfer, retention,
  mutable root publish/resolve, provider discovery, per-object transfer
  budgets, and two local `slate-settings.db` files syncing one setting through
  fixture bytes.
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
- The `slate-profile-sync` runtime bridge now verifies received typed Chat,
  Files, and Storage metadata is visible through those typed app-domain watcher
  polls after a trusted broadwebd apply. The fixture initializes receiver
  cursors before sync, applies a signed encrypted snapshot, then uses the
  typed watcher apply-and-acknowledge helper so each cursor is persisted only
  after the simulated app callback inspects the decoded payload batch. The same
  watcher path now covers post-snapshot update tails for all three domains, so
  apps can observe incremental metadata changes after acknowledging the
  snapshot batch. Chat tombstone tails are covered through the same typed
  watcher path, proving deletions are observable before the app advances its
  cursor.
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
  receive step: resolve and verify a per-device head, record the verified head
  root in `slate-settings.db`, apply the referenced settings manifest when the
  head changed, and report unchanged heads without reapplying.
- The local two-device broadwebd fixture now publishes and pulls a trusted
  signed encrypted device head through an in-process per-device root. The test
  retains the head object on the receiving provider before the publishing
  provider goes offline, records the verified head root in `slate-settings.db`,
  verifies the unchanged-root short circuit on the next pull, and follows the
  head to apply the referenced settings manifest while the publisher is offline.
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
  snapshot backend object id in storage so later compaction skips already
  squashed revisions.
- `slate-profile-sync` now has an initial local publish-flow helper that
  creates a full settings snapshot from `slate-settings.db`, publishes the
  signed encrypted snapshot manifest, publishes the local per-device head
  pointing at that manifest, and records both published roots locally. This
  gives new trusted devices a complete state handoff without requiring prior
  snapshot context.
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
  `slate://settings` instead of waiting for an internal page request.
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
  stored `settings/latest` root.
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
  storage applies enrollment or revocation.
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
  in-process availability provider. The read-only plan path remains
  membership-unaware until membership discovery has a non-mutating preview mode.
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
- The broadwebd membership-log receive fixture now covers that epoch-ordering
  rule over in-process object transfer: a stale older-epoch record is rejected
  without storing it, re-trusting the device, or advancing the membership-log
  root.

Next:

- Continue extending the protocol-neutral `profile-sync` application service
  toward real protocol-backed provider materialization.
- Continue splitting sync backends into discovery, connectivity, transfer,
  availability, and mutable-root implementations so different broadweb
  protocols can be combined behind the typed provider role records.
- Extend the stored signed membership record history into the full
  equal-control account authority model: add epoch transition rules,
  multi-approval policy, recovery-file enrollment, and provider records with no
  profile write authority.
- Add Kubo RPC-backed operations for encrypted object add, pin, unpin, pin
  verification, IPNS publish, and IPNS resolve as the first IPFS/IPNS backend.
- Implement the actual runtime sync loop that supplies a broadwebd-backed object
  source, trusted account keys loaded from `slate-settings.db`, and explicit
  sync policy checks to the storage pull-and-apply helper.
- Extend the runtime watcher beyond the first chrome settings path so
  externally synced routing, privacy, app, and browser-core changes are applied
  through their normal update paths instead of raw database replacement.
- Define the Slate Sync Secret hierarchy for manifest signing, mutable-root
  publishing, content encryption epochs, and device enrollment.
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
- Commit each coherent step separately and rerun focused regression tests for
  storage, broadwebd, rail apps, and chrome behavior as those areas are touched.

Backlog:

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
