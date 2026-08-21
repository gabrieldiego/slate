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
  while product terminology moves to Chat.
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

Backlog:

- Downloads: source URL, final routed URL, protocol or transport, filename,
  saved path, size, MIME type, status, timestamps, failure reason, and later
  integrity metadata. Download files should remain normal files; the database
  should store their records.
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
- broadwebd has a protocol-neutral `profile-sync` application service with an
  in-memory local fixture backend. Unit tests cover object transfer, retention,
  mutable root publish/resolve, provider discovery, and two local
  `slate-settings.db` files syncing one setting through fixture bytes.
- The local profile-sync fixture can now mark simulated devices offline and
  online, allowing tests to verify unavailable devices fail closed without
  touching sockets, DNS, Tor, IPFS, or external relays.
- Storage now provides `EncryptedSyncObject` envelopes using ChaCha20-Poly1305
  AEAD through `ring`, and the local two-device fixture moves encrypted setting
  change payloads through broadwebd instead of plaintext JSON.
- Storage now also provides `SignedSyncObject` wrappers using Ed25519 device
  keys, and the local two-device fixture verifies the signed encrypted payload
  against device A's trusted public key before decryption and application.
- `slate-settings.db` now has typed snapshot metadata APIs for recording
  encrypted backend object ids, covered revisions, included domains, and latest
  snapshot lookup. This is metadata only; snapshot payloads stay in encrypted
  sync objects.
- `sync_state` now has typed profile sync root APIs so storage can persist the
  last verified manifest object id for roots such as `settings/latest` without
  exposing raw key/value state to callers.
- Storage now has a serializable `ProfileSyncManifest` payload with optional
  snapshot object id, tail change object ids, included domains, and device
  frontiers. The local two-device fixture now publishes `settings/latest` to a
  signed encrypted manifest object, then fetches and applies the signed
  encrypted tail setting-change object named by that manifest.
- IPFS/IPNS is the first concrete backend under consideration, but the product
  goal is protocol-neutral: approved Slate devices should find each other, move
  encrypted sync objects, and optionally use approved providers to keep those
  objects available.
- The account model must not depend on a root server, a root node, or one
  always-hot device. Authorized logged-in devices should have equal profile
  control; hot devices and contracted providers only improve encrypted object
  availability.

Next:

- Expand the protocol-neutral `profile-sync` application service with explicit
  policy checks for richer local fixture behavior.
- Split sync backends into discovery, connectivity, transfer, availability, and
  mutable-root roles so different broadweb protocols can be combined.
- Define the equal-control account authority model: signed device heads,
  membership epochs, enrollment records, and availability providers with no
  profile write authority.
- Add Kubo RPC-backed operations for encrypted object add, pin, unpin, pin
  verification, IPNS publish, and IPNS resolve as the first IPFS/IPNS backend.
- Add snapshots, sync object metadata, conflict handling, and merge policy on
  top of the initial `slate-settings.db` typed change and revision model.
- Add a settings watcher so externally synced changes are applied through normal
  runtime update paths instead of raw database replacement.
- Define the Slate Sync Secret hierarchy for manifest signing, mutable-root
  publishing, content encryption epochs, and device enrollment.
- Publish encrypted profile manifests and snapshots through the selected
  backend, then point the profile mutable root at the newest manifest.
- Implement retention and compaction: keep deltas for active devices and recent
  changes, then squash older state into encrypted snapshots.
- Add device enrollment, revocation, and key rotation before syncing sensitive
  profile domains.
- Expand local distributed-protocol fixtures to simulate offline devices,
  delayed sync, availability loss, pinning policy, and conflicts without using
  the real internet, Tor, public IPFS/IPNS, or external relays.
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
- Add first-party app sync domains for Files, Contacts, Calendar, Chat,
  Downloads, Storage, and future rail apps. Each app should define typed change
  records, merge behavior, privacy notes, and whether it syncs content bytes,
  metadata, or both.

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
