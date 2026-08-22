# Profile Sync Over Broadweb Protocols

Status: planning

Slate profile sync should make profile state portable without turning
`slate-settings.db` into an unsafe multi-writer database. The product goal is
that every Slate device logged into the same account can safely discover,
connect to, and sync with the user's other approved devices across the
broadweb.

The sync layer uses broadwebd for protocol backends, discovery, transfer,
retention, publishing, pinning where applicable, and provider health. Slate
storage and browser-core own settings semantics, merge policy, account consent,
and runtime application of updates. IPFS/IPNS is the first concrete backend
under consideration, not a hard product dependency.

## Goals

- Start with `slate-settings.db` as the first local materialized view for
  distributed profile state.
- Sync selected profile state across trusted Slate devices.
- Avoid any root node, central account server, or single always-hot device that
  can dictate profile state.
- Give all authorized logged-in devices the same level of profile control within
  the current account policy.
- Store only encrypted profile payloads in broadweb transfer and storage
  backends.
- Use a signed mutable root to locate the current sync manifest. IPNS is one
  possible root backend, but not the only acceptable design.
- Let logged-in devices retain the profile data they need.
- Support optional contracted or self-hosted providers that retain encrypted
  data for availability without being trusted to validate profile state.
- Keep remote pinning, public gateways, relays, discovery, and provider behavior
  explicit.
- Keep the same application service usable by multiple protocols.
- Keep implementation progress recoverable through small commits and rerun
  focused regression tests after each step.

## Non-Goals

- Do not sync the raw SQLite database as authoritative multi-writer state.
- Do not publish plaintext settings, bookmarks, history, cookies, or secrets.
- Do not expose the Kubo RPC API outside loopback.
- Do not require full DHT provider behavior by default.
- Do not assume any discovery protocol guarantees data availability.
- Do not make a central server or one always-online device the source of truth.
- Do not give contracted pinning, relay, discovery, or storage providers profile
  write authority.
- Do not sync cookies, site storage, passwords, or private-window state until
  separate privacy designs exist for those domains.

## Current Baseline

broadwebd already has an IPFS/IPNS retrieval path for browsing through a local
gateway and an opt-in Kubo RPC fetch path. That is useful for navigation, but
profile sync needs a broader service model:

- Store encrypted sync objects through one or more backends.
- Fetch encrypted sync objects by backend-specific object id.
- Retain, release, list, and verify profile sync objects.
- Publish and resolve signed mutable roots for profile manifests.
- Discover approved devices and providers that may have the data.
- Connect to devices through direct, relay, local, or private-network paths.
- Track local backend health and availability.
- Expose an approved application service to Slate storage code.

The first IPFS-backed implementation can use Kubo RPC on loopback because Kubo
already exposes the required add, pin, and name APIs. broadwebd must validate
that configured RPC endpoints are local unless the user explicitly configures a
reviewed remote service. Sync writes must not fall back to a public gateway.

## Protocol Roles

Profile sync should split protocol responsibilities instead of expecting one
network to solve everything:

```text
Discovery:
  Find approved devices or providers that may be online.

Connectivity:
  Establish an encrypted channel to a discovered device or provider.

Transfer:
  Fetch or push encrypted sync objects.

Availability:
  Keep encrypted objects online through logged-in devices, a home daemon, or
  contracted/self-hosted providers. Availability providers retain bytes; they do
  not get profile signing authority.

Mutable root:
  Locate current signed sync manifests or device heads.
```

Different protocols can fill different roles. For example, Iroh can improve
live device discovery and direct or relayed transfer; IPFS can provide
content-addressed objects and pinning; IPNS can publish a mutable profile root;
Tor onion services can make a home daemon privately reachable; and a contracted
pinning service can improve encrypted object availability without being trusted
with plaintext.

The initial Files projection uses `slate-settings.db` as a metadata-only
materialized view keyed by file entry id. Replicated Files payloads may include
sync-set membership, parent entry, display name, entry kind, content object
reference, MIME type, size, modified time, integrity, and retention policy. They
must not include file bytes, local filesystem paths, or per-device availability
state. Those heavier and device-specific concerns belong in later
object-transfer and retention layers.

## Account Authority Model

A Slate sync account is a replicated cryptographic authority set, not a server
login and not a root device. The account should be identified by stable
cryptographic material and a signed membership history. Every logged-in device
has its own device keypair and writes signed changes under its own device head.

Baseline rules:

- No central service decides the current profile state.
- No single always-online device is required for control.
- Any authorized logged-in device can publish its own signed head and participate
  in merges.
- The current profile view is the verified merge of authorized device heads,
  snapshots, and retention metadata.
- A hot desktop, home daemon, or contracted provider may retain encrypted data
  and advertise availability, but cannot add devices, revoke devices, decrypt
  data, or forge profile changes.
- If only one device or provider is online, Slate may still sync through it for
  availability, but the authority remains in the signed device set.

Device enrollment should use an existing authorized device, a recovery file, or
a threshold recovery flow. The first implementation may allow one authorized
device to approve a new device, but the format should leave room for stricter
policies such as M-of-N approval for device revocation, recovery, or sensitive
sync domains.

`slate-settings.db` now has the first local persistence primitive for that
authority set: signed membership records are stored by profile, record id,
membership epoch, record kind, target device id, signer device id, and exact
signed bytes. The storage helper verifies the record signature against the
embedded signer key and validates the signed payload shape before insert. A
separate local apply helper can bootstrap the first self-signed device
enrollment, then requires later records to verify against a currently trusted
signer key before enrolling, revoking, or rotating a target device key. Applied
membership records are marked once so replaying an older enrollment record does
not re-trust a later-revoked device.

## State Model

The local SQLite database remains the fast materialized view that the browser
uses at runtime. The sync state is a typed, signed change log plus periodic
encrypted snapshots:

- `settings_values` stores the current local value for each synced setting.
- `settings_changes` stores append-only local and remote change records.
- `settings_snapshots` tracks compacted encrypted profile snapshots by backend
  object id.
- `sync_state` tracks mutable roots, known devices, frontiers, and publish
  state.
- The storage pull boundary can list visible mutable-root candidates, verify
  each candidate's signed manifest, and apply verified candidates in
  deterministic oldest-to-newest publication order. Per-setting conflict
  resolution still uses the typed change policy, not backend publication order.
- Runtime-style pulls can use the active content-key epoch and report missing,
  unchanged, or applied candidate sets without fetching objects when the newest
  visible candidate already matches the stored verified root.
- `settings_revisions` lets runtime services observe committed changes.

External updates should enter through validated change records and snapshots,
not by replacing the live SQLite file or replaying raw WAL pages. Runtime code
subscribes to typed settings events and applies them through normal
browser-core, chrome, routing, and privacy configuration paths.

## Object And Root Shape

The transfer backend stores immutable encrypted profile objects:

- Snapshot objects contain a compacted profile view for approved domains.
- Change objects contain one signed operation from one device.
- Attachment objects can be added later for larger profile artifacts.

The mutable-root backend stores or advertises current profile roots or device
heads:

- The root resolves to the latest encrypted sync manifest object id.
- Backends may expose multiple visible root candidates for the same root when
  equal-control devices publish competing manifests.
- The manifest identifies the current snapshot and unsquashed tail changes.
- The manifest is signed by an authorized device key or by an account policy
  threshold.
- Publish-side storage helpers build tail-change manifests from local change
  records and backend object ids so device frontiers and included domains come
  from typed `slate-settings.db` state, not ad hoc caller assembly.
- Snapshot publish helpers do the same for compacted snapshot manifests,
  preserving compacted device frontiers and allowing a retained manifest tail
  to extend the newest device frontier.
- Each device verifies signatures before applying any sync data.

This makes backend persistence independent from trust. Object identifiers, such
as IPFS CIDs or Iroh BLAKE3 hashes, identify bytes. Slate signatures and
encryption decide whether those bytes are authorized profile state.

The initial local fixture path uses storage-owned `EncryptedSyncObject`
envelopes and `ring`'s ChaCha20-Poly1305 AEAD to keep sync object payloads
opaque before broadwebd stores or transfers them. Public envelope metadata such
as profile, domain, object kind, and key id is authenticated as associated data.
Storage also provides a `SignedSyncObject` wrapper using Ed25519 device keys so
the receiving side can verify encrypted object bytes against an already trusted
device public key before decrypting or applying them. This is the first
device-signature primitive only; account membership epochs, manifest
signatures, recovery credentials, revocation, and key rotation remain separate
sync-layer work.

`slate-settings.db` stores profile-scoped trusted device signing keys and the
settings pull path now uses that store to verify each signed manifest, snapshot,
and tail object. The embedded public key in a sync object is treated as
untrusted metadata until it matches the stored key for that signing device.
Unknown devices, stored-key mismatches, and signer keys first trusted after the
manifest membership epoch fail before decryption or root advancement.

## Key Model

The shareable recovery credential should be a Slate Sync Secret, not one raw key
used for every purpose and not a normal day-to-day device credential. From that
root secret, derive separate material for:

- Account identity and recovery.
- Device enrollment approval.
- Mutable-root publish delegation, where a backend requires it.
- Content encryption key epochs.
- Device identity bootstrapping.

Each device should have an individual device signing key authorized by the
account membership history. Sharing one mutable-root publishing key with every
device is simple, but it makes revocation harder and should not become the
account authority model. If a backend requires a simpler first step, document
that limitation and keep the interface capable of supporting per-device heads or
delegated publishing later.

Key rotation and device revocation should be represented as account membership
epoch changes. Old epochs may remain readable for migration, but new writes
should use the current encryption epoch.

`slate-settings.db` tracks content-key epoch metadata so runtime code can bind
objects to the expected key id, membership epoch, algorithm, and active epoch.
The active-key pull path rejects unsupported algorithms and keys introduced
after the manifest membership epoch before applying profile state. It does not
store raw content encryption keys. Key bytes should come from a keychain entry,
recovery-secret derivation, or an enrollment flow before being passed into the
decrypt/apply path.

The local trust store can also mark a stored device signing key as distrusted.
Runtime trusted-device enumeration skips distrusted remote keys, trusted object
opening rejects signed payloads from distrusted devices, and local credential
preflight refuses to publish with a distrusted local signer. This is a local
revocation guard for current fixtures and scheduler policy. The local
membership apply helper can now distrust a target device key from a signed
revocation record, and it rejects a different older-epoch membership operation
for that target device once a newer operation has already applied. It also
rejects a different same-epoch operation for a target device when another
record at that epoch has already applied, so a conflicting rotate cannot undo
an applied revoke in the same epoch. Exact record replay remains idempotent.
Full account-level revocation still needs richer epoch transition and
multi-approval policy.
The profile-sync membership-log receive path exercises the same storage guard:
a stale older-epoch record carried through broadwebd fixture objects fails
without storing that record, re-trusting the device, or advancing the
membership-log root.

The active-key pull path also exposes an idempotent root-status helper for sync
polling. It resolves the published root first and reports missing roots,
unchanged already-verified roots, or applied manifests separately. If the
published root object id matches the locally stored verified root, storage does
not fetch, decrypt, or apply the object set again.

Runtime watchers consume applied settings through bounded event feeds. The
general feed remains available for sync internals, but app/runtime dispatch
should prefer the domain-scoped feed so chrome, Calendar, Contacts, Downloads,
and future apps only inspect replicated payloads for the sync domain they own.
Watchers should initialize their cursors from the latest applied revision for
their own domain instead of using the profile-wide latest revision. The storage
poll helper returns the previous cursor, latest cursor, and bounded applied
event batch for one domain, which lets app watchers share one local-only polling
contract while dispatching through app-owned update paths.
For typed app domains, storage also provides a decoded poll helper that turns
the JSON payload into an app-owned type before the caller records the cursor.
Decode failures leave the persisted cursor unchanged, so a malformed replicated
payload cannot be acknowledged as applied by accident.
Runtime code can use storage's typed app-domain watcher wrapper for the same
contract: initialize at the domain head, poll bounded decoded batches, and
acknowledge only after the app has applied the batch.
The broadwebd runtime bridge fixture now covers this watcher contract after a
trusted receive: Calendar, Chat, Contacts, Downloads, Files, and Storage
cursors are initialized before sync, the signed encrypted snapshot is applied,
and each typed watcher runs an app callback over the decoded payload before
acknowledging the cursor.
The update-tail fixture then records those snapshot cursors, applies a
post-snapshot manifest tail, and verifies each app-domain poll returns only the
incremental decoded payload for that domain.
The tombstone-tail fixture uses the same cursor sequence for Calendar, Chat,
Contacts, Downloads, Files, and Storage deletions, so apps can observe decoded
deletion payloads and only then acknowledge the tail revision.

## Data Objects

The exact encoding can be CBOR, postcard, or another compact structured format.
The logical shape should remain explicit:

```text
sync_manifest:
  profile_id
  profile_epoch
  schema_version
  current_snapshot_object_id
  tail_change_object_ids
  authorized_devices
  device_frontiers
  membership_epoch
  retention_policy
  created_at
  signature

device_head:
  profile_id
  device_id
  root_id
  schema_version
  membership_epoch
  latest_manifest_object_id
  latest_change_object_id
  device_sequence
  logical_clock
  created_at
  signature

sync_snapshot:
  snapshot_id
  covers_frontiers
  included_domains
  encrypted_payload
  signature

sync_change:
  change_id
  parent_snapshot
  device_id
  device_sequence
  logical_clock
  domain
  operation
  encrypted_payload
  signature
```

The storage crate already represents device heads as signed encrypted sync
objects for the settings domain. Opening a trusted device head verifies the
stored device public key, decrypts the payload, checks that the payload device
matches the signer, and rejects signer keys introduced after the head
membership epoch. Unsupported device-head schema versions fail closed at decode
time. Device heads can be pulled through the same profile-sync object source
abstraction as manifests: the helper resolves the per-device head root, fetches
the object, verifies/decrypts it, requires the decrypted payload root id to
match the resolved root, and returns the verified head with its backend object
id. `slate-settings.db` can also record the verified head root and report
missing, unchanged, or updated status; unchanged roots skip object fetch and
decrypt work. Once a head is verified, storage can follow its referenced
manifest object id, verify/decrypt that manifest's snapshot and tail objects,
require the manifest membership epoch to match the head epoch, require a
matching manifest frontier for the head device, sequence, and latest change
object, and apply the settings manifest without resolving the global settings
root.
Publishing per-device heads is now part of the `slate-profile-sync` runtime
bridge: it validates the storage-owned head payload against the target
per-device root and signing key, signs and retains the encrypted `device-head`
object, and publishes roots such as `settings/devices/<device>/head`. Merging
multiple authorized heads is a separate runtime step. The local two-device
fixture already exercises the first handoff: a publishing provider writes a
signed encrypted head, the receiving provider retains it, and the head remains
pullable after the publisher is marked offline. The receiver records the
verified head root in `slate-settings.db` and then verifies the unchanged-root
short circuit on the next pull. The same fixture also retains the referenced
manifest and tail
objects, then applies the settings manifest by following the verified head while
the publishing provider remains offline.

The first synced domains should be settings that are safe to apply live, such as
UI preferences, protocol adapter configuration, rail app ordering, and
bookmarks. More sensitive domains need separate threat notes before entering
sync.
The initial chrome runtime path watches the typed applied-settings event feed
from `slate-settings.db` and advances a local revision cursor during the normal
desktop Servo pump. Recognized chrome settings such as chrome zoom and key
bindings are applied through the existing in-memory runtime setters, so synced
changes no longer depend on serving a `slate://` page. Later routing, privacy,
browser-core, and app settings should plug into the same watcher shape instead
of replacing raw database state behind active components.

## App Sync Domains

Every first-party Slate app should use the same profile-sync substrate when its
state is meant to follow the user across logged-in devices. App sync must remain
typed and domain-specific: each app owns its merge rules and privacy notes, but
it stores changes as signed encrypted `sync_change` records and compacted
snapshots under the shared account authority model.

Planned domains:

- `settings`: browser settings, key bindings, rail layout, protocol adapter
  policy, and other low-risk preferences.
- `bookmarks`: home slots, bookmark folders, ordering, tags, notes, and favicon
  references.
- `calendar`: events, calendars, reminders, recurrence metadata, and provider
  mapping. Calendar data is sensitive and must be encrypted before any backend
  sees it. The initial Calendar projection stores event metadata locally and
  emits JSON changes keyed by event id, including tombstones for deletion.
  Calendar remains disabled by default so those sensitive values are only
  published after the user enables the domain and profile-sync seals the changes
  into encrypted objects.
- `contacts`: contact cards, identities, groups, local aliases, and provider
  mapping. Contacts are sensitive. The initial Contacts projection stores basic
  contact cards locally and emits JSON changes keyed by contact id, including
  tombstones for deletion. Contacts remains disabled by default so names, email
  addresses, phone numbers, notes, and avatar references are only published
  after the user enables the domain and profile-sync seals the changes into
  encrypted objects.
- `chat`: account/provider configuration, local conversation metadata, and
  aggregation preferences. The initial Chat projection stores conversation
  metadata keyed by conversation id and emits tombstones for deletion. It may
  include provider ID, provider thread ID, display name, avatar reference,
  last-message timestamp, unread count, archive state, and mute state. Message
  contents, SMS/WhatsApp secrets, provider tokens, and attachment bytes require
  separate designs before sync.
- `files`: file metadata, user-selected sync sets, directory manifests, content
  object references, integrity metadata, and retention policy. The initial
  Files projection stores metadata-only entries keyed by entry id and emits
  tombstones for deletion. File bytes, local paths, and per-device availability
  remain outside replicated settings payloads and should use heavier
  object-transfer and retention backends later.
- `downloads`: download history, source routing metadata, integrity metadata,
  and user-selected persistent file records. Temporary downloads should stay
  local unless explicitly promoted. The initial Downloads projection is
  metadata-only: winning sync changes materialize URL, route, transport,
  filename, MIME type, byte count, and integrity fields into local rows, while
  file bytes and local paths stay out of replicated settings payloads.
- `storage`: broadweb storage providers, pinning policy, quota hints, retained
  object limits, and repair preferences. The initial Storage projection stores
  provider metadata keyed by provider id and emits tombstones for deletion. It
  may include provider kind, display name, endpoint reference, broadweb role
  flags, quota hints, retained-object limits, pinning policy, and enabled state.
  Provider credentials, private keys, local daemon paths, live health, and
  per-device availability must stay outside replicated settings payloads.
- Future apps such as Player, Notes, Tasks, Mail, or Media Library should define
  their own sync domain before storing replicated state.

Visible rail app sync-domain ownership should remain one-to-one. Web owns
`bookmarks`; Settings owns `settings`; seeded future domains such as `storage`
must not shadow a visible rail app until their UI surface exists.

The initial Bookmarks-domain projection covers home bookmark slot saves as
structured JSON text changes keyed by slot. Default first-run bookmarks are
local seed data and should not be published as user bookmark changes. Trusted
incoming bookmark slot changes materialize into local bookmark rows during
profile-sync apply. Existing bookmark removals emit tombstone payloads for the
affected home slot so receiving devices can delete stale rows without syncing
file or cache data.

`slate-settings.db` seeds these domains with privacy metadata so the sync UI can
show what each app would share before the app has its full schema. Settings,
bookmarks, and downloads metadata are enabled by default because they are
low-risk or metadata-only. Calendar, contacts, chat, files, and storage are
registered but disabled by default because they are sensitive or content
bearing. Reopening the database must not reset an existing enable/disable
choice; default seeding preserves user-controlled enablement while refreshing
the built-in domain metadata. Local publishing must consult this table before
creating snapshots or tail manifests: disabled domains may remain local typed
state, but they are not included in outgoing broadweb profile-sync objects until
explicitly enabled. Compaction target selection uses the same enabled-domain
set, so local changes in disabled domains do not force snapshot publication or
tail trimming. Publisher-side event selection should request per-domain feeds
for the enabled domains instead of reading every app-domain payload and
discarding disabled domains afterward.
Fixture coverage includes typed Chat metadata written while Chat sync remains
disabled. That state stays in the publisher's local materialized rows and is
not included in snapshot or tail publication until the domain is enabled.

No app should bypass `profile-sync` with an ad hoc network path for replicated
profile state. If an app needs a protocol-specific backend, broadwebd should
expose it through the same discovery, connectivity, transfer, availability, and
mutable-root roles.
The current runtime bridge coverage includes signed encrypted full-snapshot
handoffs for Calendar, Chat, Contacts, Downloads, Files, and Storage metadata.
It also covers tombstone snapshots that remove stale Calendar, Chat, Contacts,
Downloads, Files, and Storage typed rows from a receiver, post-snapshot update
tails for Calendar, Chat, Contacts, Downloads, Files, and Storage, and
Calendar/Chat/Contacts/Downloads/Files/Storage tombstone tails to verify
incremental typed changes use the same trusted device-head path. These tests
use `InProcessBroadwebNetwork`, so app-domain create, update, and delete
propagation is verified without
loopback sockets or external protocols.

## Compaction And Retention

Slate should keep deltas long enough for active devices to sync efficiently, but
it must not keep an unbounded change log because a device vanished.

Initial heuristic:

- A device is active if it is authorized and has been seen within the retention
  window.
- Keep changes needed by active devices.
- Also keep all changes newer than the minimum age window.
- Periodically write a new encrypted snapshot that covers older changes.
- Remove old changes from the manifest tail after the snapshot is published and
  pinned by the current device.
- A timed-out device rejoins from the newest snapshot and rebases any local
  unsynced changes it still has.

The manifest should record enough device frontier information to make compaction
auditable without requiring every device to be online.

## broadwebd Service Boundary

broadwebd should expose a `profile-sync` application service. Slate storage code
asks for approved sync operations; it does not call protocol internals directly.

Required service operations:

- Put an encrypted object and return a backend object id.
- Fetch an encrypted object by backend object id.
- Retain, release, list, and verify objects used by profile sync.
- Publish a mutable root to a manifest object id.
- Resolve a mutable root to a manifest object id.
- List competing mutable-root candidates when multiple authorized devices have
  published roots that may need merge handling.
- Discover approved devices and providers that may have profile sync objects.
- Establish direct, relayed, local, or private-network transfer sessions.
- Report backend health and publish failures.
- Stream sync availability events to Slate.

The first implementation should use a fake backend in unit tests. Kubo RPC can
be the first IPFS/IPNS backend for manual or ignored integration tests. Iroh,
Syncthing-like device providers, Tahoe-LAFS-style storage, Tor-reachable home
daemons, and non-IPFS backends should fit behind the same application service
contract when they prove useful.

The `slate-profile-sync` crate owns runtime-facing sync glue that depends on
both broadwebd and storage. Its first adapters implement storage's
`ProfileSyncObjectSource` trait for a `BroadwebDaemon` and provide a small
publisher for putting encrypted objects, retaining objects, publishing mutable
roots, and checking retention state. Storage stays protocol-neutral, broadwebd
stays independent from storage's encrypted object semantics, and sync-only tests
should target this crate when possible so they do not compile the renderer.
The publisher also has a retained dependency/root helper for publish flows that
must upload snapshot or tail objects before publishing the manifest root.
For settings tails, the bridge can convert local `SyncChangeRecord` values into
signed encrypted `setting-change` objects, build the storage-owned manifest,
and publish that manifest root without taking ownership of merge policy or
database mutation.
For settings snapshots, the bridge can sign and retain the compacted snapshot
object, sign and retain any post-snapshot tail objects, ask storage to build the
snapshot-and-tail manifest, then publish the mutable root to the signed
manifest object. Compaction policy and snapshot payload selection remain owned
by storage.
The bridge can also drive one storage-selected compaction step: ask
`slate-settings.db` for a compaction target, derive snapshot domains from the
covered change records, publish the signed snapshot manifest, and record the
published snapshot object id back into storage.
For per-device heads, the bridge signs and publishes storage-owned
`ProfileSyncDeviceHead` payloads while keeping the head schema and trust checks
in storage.
For account membership, the bridge can publish signed storage-owned membership
records under explicit `account/membership/<record-id>` roots and receive those
records by resolving broadwebd roots, fetching retained objects, and applying
them through `slate-settings.db`. The fixture coverage for this path uses two
devices on one `InProcessBroadwebNetwork`, so membership authority propagation
is exercised without loopback listeners, DNS, public gateways, Tor, IPFS/IPNS,
or external relays.
The bridge can now also publish a small `account/membership/log` index that
lists retained membership record objects in epoch order. The index is only a
discovery layer: receivers validate each entry against the signed membership
record payload and rely on storage authorization before changing trusted device
state.
An additive runner path can pull and apply that membership log before running
the normal settings cycle. This lets a newly enrolled local device learn its
own trusted key and remote trusted keys before credential preflight and before
trusted device-head pulls.
The published membership log exposes the retained object ids for both the log
and its referenced signed records, and the broadwebd publisher can hand that
object set to an availability provider. Fixture coverage retains the set
through an in-process provider so no loopback socket or external pinning service
is needed.
The membership-aware settings runner can also hand the combined settings and
membership publication set to selected availability providers. This keeps the
bootstrap authority records and the first settings objects available through
the same in-process retention path.
The active-key preflight also has a membership-aware form: it pulls the
membership log before checking the local signer against trusted device keys.
This gives scheduler planning a path for newly enrolled devices whose local
trusted key arrives from the distributed membership history.
The scheduler facade now has an explicit membership-aware selected-provider run
path. It pulls membership, performs active-key preflight, filters selected
retention-provider handles against discovered providers, runs the settings
cycle, and retains the combined settings and membership publication set through
selected in-process providers. The read-only scheduler plan path remains
membership-unaware for now because pulling membership mutates
`slate-settings.db`.
The receive fixture also covers a tampered index entry that points at a
different signed record object. That path fails before `slate-settings.db`
stores the membership-log root or writes any trusted device key.
Membership logs are capped at a fixed record count before entry fetch or apply,
so oversized indexes fail before they can force unbounded fixture object reads
or database mutations.
The publisher enforces the same cap through that count-only plan before loading
signed membership record blobs, writing membership record objects, or advancing
the log root, so oversized local history waits for future membership compaction
instead of publishing an index receivers must reject.
A read-only publication plan exposes empty, publishable, and too-large states
from a count-only `slate-settings.db` query without loading every signed
membership record blob or touching broadwebd. The scheduler facade can compose
that local membership-log preview with the existing read-only selected-provider
plan, giving runtime/UI code a safe membership-aware preview before membership
compaction exists.
Membership-aware runner and scheduler runs also check that local publication
plan first and refuse oversized local history before pulling remote membership,
publishing settings objects, or advancing roots.
The broadwebd source bridge can also run the receive side for one trusted
device head: resolve and verify the head, record the verified head root in
`slate-settings.db`, apply the referenced settings manifest when the head is
new, and return an unchanged status when the stored head root is already
current.
The first composed local publish helper emits a complete settings snapshot,
publishes the snapshot manifest, publishes the local per-device head pointing at
that manifest, and records both published roots locally. This favors a complete
handoff for new trusted devices before the later incremental publish loop trims
tails and reuses retained snapshots.
The next publish helper reuses the latest retained local
`slate-settings.db` snapshot object when new settings changes appear after that
snapshot. It rebuilds the covered snapshot payload from storage for validation,
retains the existing backend object, publishes only the post-snapshot tail
changes, moves the settings mutable root to the new manifest, publishes a fresh
local device head, and records both roots locally. Tests run the handoff between
two simulated devices through `InProcessBroadwebNetwork`, so this path does not
bind loopback sockets or rely on external IPFS/IPNS, Tor, DNS, or relay
services.
The scheduler-facing local publish entry point now chooses between that
incremental tail path, the full-snapshot bootstrap path, and explicit no-op
states by inspecting `slate-settings.db`. This keeps the eventual scheduler from
duplicating assumptions about whether the latest snapshot was retained, whether
new rows exist after it, or whether a profile has no syncable settings yet.
The first local scheduler loop is intentionally bounded and single-threaded:
callers provide the maximum number of publish steps, and the loop stops at
`NoLocalSettingsChanges` or `UpToDate`. Before publishing a post-snapshot tail
it decodes the stored local device-head object through broadwebd and compares
that signed frontier with the latest local-device setting sequence. Applied
remote-device rows after the retained snapshot are not re-signed into the local
tail; future merge work should publish those devices through their own trusted
heads or through an explicit merged snapshot.
The receive side has a matching bounded trusted-device runner. It lists
profile-scoped trusted device public keys from `slate-settings.db`, skips the
local sync device id, rejects runs that exceed the caller's device limit, and
pulls each trusted settings device head via the broadwebd source bridge. The
runner reports per-device results so callers can distinguish applied manifests,
unchanged heads, and trusted devices that have no published root yet.
The same broadwebd source bridge also exposes storage's active trusted
settings-root candidate path. A receiver can list all visible competing
`settings/latest` roots through broadwebd, verify each signed manifest against
trusted device keys and the active content-key epoch in `slate-settings.db`,
apply them in storage's deterministic candidate order, and record the newest
verified root. This keeps equal-control device merge behavior available through
the runtime-facing bridge without adding a socket fixture or a backend-specific
merge path.
A higher-level settings sync cycle composes the local publisher and receive
runner. It preflights the trusted-device limit, publishes local pending settings
first, then receives from trusted devices. The local device's own public key
still belongs in the trusted-key table because manifests may reference retained
snapshot dependencies signed by the local device, even though the receive runner
does not pull the local device's head as a remote source.
An active-key policy cycle variant can also apply visible shared settings-root
candidates after the bounded local publish and device-head receive steps. Its
result reports the normal cycle plus the shared-root candidate status, so the
runtime scheduler can distinguish "no device head changed" from "equal-control
shared roots were merged" without adding another transport or polling loop. A
receive-only candidate merge can recover the shared settings root while still
leaving the local device-head root missing; strict after-cycle local-head
health remains a separate policy choice.
Settings manifest application reports the verified sync object ids it consumed:
manifest, optional snapshot, and tail changes. The shared-root cycle exposes
those received candidate object ids, allowing a scheduler to ask availability
providers to retain merged roots and their dependencies after receive-side
verification while keeping those providers outside mutable-root authority.
Before the cycle touches broadwebd it validates the caller-supplied credentials
against local profile state: `key_id` must match the active content-key epoch in
`slate-settings.db`, that epoch must use the supported content encryption
algorithm, and the supplied local signer must match the trusted public key for
the local sync device. The database stores key metadata and trusted public keys,
not plaintext content keys or private signing keys.

The local fake backend must model provider availability inside the test process.
Each simulated device registers as a provider, retained objects are scoped to
that provider, and discovery only reports providers that the fixture currently
marks online. This lets tests exercise pinning and availability policy without
loopback sockets, OS DNS, public gateways, Tor, IPFS/IPNS, or external relays.
Provider discovery reports explicit roles for discovery, connectivity, object
transfer, availability, and mutable-root publishing; the older
`can_publish_roots` flag remains a compatibility view over the mutable-root
role. This keeps availability-only providers from being mistaken for devices
with profile write authority. The local fixture enforces those roles before
serving object transfer, retention, provider discovery, root discovery, or
mutable-root publish requests, and objects held by providers without the
object-transfer role are not visible to other simulated devices. The fixture can
also block retention for a selected provider through a simulated local pinning
policy while leaving that provider online and able to transfer encrypted
objects. This lets tests model "provider reachable, but not willing to pin"
without binding sockets, sleeping, or changing provider roles. It can also cap
retained object count per provider, so tests can model quota exhaustion and
quota recovery after release without conflating those states with offline
availability or transfer failure.
The service boundary validates profile ids, mutable-root ids, and backend
object ids before any fixture backend lookup, retain, or publish operation.
Malformed identifiers fail locally instead of being interpreted as path-like
state, implicit URLs, or backend-specific fallthrough.
Provider health requests summarize known, online, offline, fresh, stale,
object-transfer, availability, and mutable-root providers inside the fixture and
mark the profile degraded when one required role has no fresh online provider.
Freshness uses an explicit in-process sequence floor controlled by the fixture,
not wall-clock sleeps, sockets, or background network polling. Root health
requests inspect a concrete mutable root, reporting visible candidates, latest
root object availability, whether that object is retained by a fresh online
provider, and whether it meets the caller's minimum online retaining-provider
quorum.
The `slate-profile-sync` bridge exposes those checks as one read-only settings
sync health report for the eventual scheduler: provider health, shared settings
root health, and local device-head root health are gathered through the selected
broadwebd daemon without publishing, pulling, or opening any additional
transport. Fixture tests use `InProcessBroadwebNetwork` for this path so health
policy can be validated without loopback ports, OS DNS, public gateways, Tor,
IPFS/IPNS, or external relays.
The runner can also return before-and-after health around one bounded settings
sync cycle. That keeps the future runtime scheduler's first responsibility
simple: run a capped cycle, inspect whether provider/root health changed, and
surface degraded state without introducing background polling, socket fixtures,
or a separate health transport.
Cycle policy is now explicit data: retention policy, maximum local publish
steps, maximum trusted devices, minimum online retaining-provider quorum, and
whether fresh provider health is required before running. The runtime-facing
policy path samples health first and rejects degraded provider roles before
loading credentials or attempting mutable-root writes, while still allowing
missing settings roots to recover during an initial healthy-provider publish.
The same policy can require minimum fresh online, object-transfer,
availability, and mutable-root provider counts, and it can cap stale online
or offline providers when the scheduler wants stricter freshness and
availability. These thresholds are Slate scheduler decisions over broadwebd's
reported health; broadwebd stays a protocol-neutral reporter and fixture host.
After the bounded cycle runs, the policy-gated path checks whether the settings
root and the local device-head root satisfy the configured online
retaining-provider quorum. This lets a first publish recover from missing roots
while still surfacing insufficient post-publish availability as a runtime policy
failure.
Published cycle results expose the encrypted object ids created by the local
publish step: settings snapshots, tail changes, manifests, and device-head
objects. Availability providers can retain that exact object set through
broadwebd without gaining mutable-root authority. Fixture coverage uses an
`InProcessBroadwebNetwork` availability-provider daemon to copy those objects
inside the test process and then re-check root health, so quorum recovery is
validated without loopback sockets, OS DNS, Tor, IPFS/IPNS, public gateways, or
external relays.
The active-key policy runner can perform that retention handoff as part of the
same bounded cycle: after local publish and trusted-device receive, it asks each
supplied availability-provider daemon to retain the published object ids and
returns a per-provider status report before enforcing the post-cycle root
health policy. This is still a local fixture shape, but it gives the future
runtime scheduler the right sequencing contract: publish encrypted state,
handoff availability, then evaluate whether the profile is durable enough.
The same runner also has a shared-root candidate variant for equal-control
devices: after the local publish/device-head receive steps, it applies visible
trusted `settings/latest` candidate manifests, retains the union of locally
published objects and verified received candidate objects, and then checks root
health. A receive-only shared-root recovery can relax local device-head health
while still requiring the shared settings root itself to satisfy the configured
retaining-provider quorum.
After provider policy passes, the runtime-facing runner can load the active
content-key id from `slate-settings.db` and use caller-supplied secret material
for the actual encrypted publish/pull cycle. The database therefore stores
active key metadata and trusted public keys, not plaintext content keys or
device signing keys.
The local device signer must match the database's local sync device id before a
cycle can publish local device-head state. Trusted remote public keys remain
valid receive-side trust anchors, but they cannot be reused as the local
publishing identity.
The runner exposes that work as an explicit read-only preflight: sample
broadwebd health, apply before-cycle provider policy, load active key metadata,
validate the local signer, and enforce trusted-device bounds. Preflight does
not take the content key secret and does not publish, retain, pull, or mutate
sync roots. It also returns discovered online retention-capable providers with
availability and object-transfer roles. Freshness is enforced through the
provider health policy, while the provider records give the scheduler concrete
selection candidates without granting those providers mutable-root authority.
The health report now includes concrete fresh, stale, and offline provider ids
from the in-process fixture. Scheduler handle selection uses those ids and the
discovered provider role records to distinguish stale, offline, and
role-ineligible selected retention providers from unknown providers, and the
runtime path rejects those selected providers before publishing, pulling,
retaining objects, or mutating sync roots.
The first scheduler facade is deliberately explicit: one caller-triggered tick
combines a profile/root config, caller-held content key and signer, the local
broadwebd daemon, and selected retention-provider daemons. It runs the
active-key shared-root candidate cycle, hands the verified object-id set to the
selected providers, and returns the health/retention result. A second tick path
accepts provider-id/daemon handles and filters them against preflight's
discovered retention-capable providers before retaining objects, so scheduler
tests can model provider selection through `InProcessBroadwebNetwork` without
loopback sockets or external discovery. That selector is also exposed as a
read-only scheduler plan: it runs preflight, reports selected, undiscovered,
and duplicate handles, and does not publish, pull, retain, or mutate sync
roots. The scheduler can also derive that read-only plan from enabled
storage-provider metadata in `slate-settings.db`: stored providers must locally
advertise object-transfer and availability before they are compared with
broadwebd discovery, and disabled, locally role-ineligible, stale, offline,
broadweb-role-ineligible, and undiscovered providers are reported separately.
That gives runtime/UI code a bounded provider-materialization preview without
starting protocol daemons, publishing, retaining, or opening sockets. A stored
provider scheduler tick can then run only against already-materialized daemon
handles supplied by runtime code. Stored selected providers without a matching
handle are reported as unmaterialized, and only materialized selected providers
count toward the retaining-provider quorum. When a stored row has an
`endpoint_ref`, the materialized handle must report the same endpoint before it
can be used for retention. Endpoint mismatches are tracked separately from
missing handles and excluded from quorum, so runtime code cannot satisfy a
stored provider selection with the wrong local fixture or future protocol
endpoint. The stored-provider plan also classifies endpoint references before
runtime materialization: `InProcessBroadwebNetwork` mints
`slate-fixture-profile-sync://<network>/<provider>` endpoint refs for
socketless profile-sync tests. The scheme and prefix are exported from
broadwebd along with a pure parser/validator, so profile-sync classification,
future materialization code, and fixture minting share one source of truth.
Those refs are treated as in-process fixture endpoints only when the parser
accepts the shape and the provider component matches the stored provider id.
Multiaddr-like and deferred protocol references are reported separately, and
ordinary `http://`, `https://`, `localhost`, or stale fixture-shaped references
are reported as unsupported. This keeps test fixtures from drifting back to
loopback listeners or DNS-backed URLs before any daemon startup code runs.
The stored-provider plan exposes provider-id buckets and counts for each
endpoint status: in-process fixture, missing, multiaddr, deferred protocol, and
unsupported. That gives runtime and UI code a structured preview of which
providers can use local fixtures, which need future protocol materialization,
and which fail closed before any provider daemon is started. The same buckets
are available for only the selected retention providers after preflight, so
runtime code can distinguish selected fixture providers from selected future
protocol endpoints without re-filtering disabled, stale, offline, or
undiscovered providers.
Selected endpoint buckets are also folded into a compact materialization
preview: fixture-ready providers can run against local in-process handles,
missing, multiaddr, and deferred-protocol providers remain pending
materialization work, and unsupported providers fail closed.
For selected synthetic fixture endpoints, the plan also exposes
materialization targets carrying the provider id, fixture network id, and
endpoint ref. This lets local-only test fixtures bridge from stored metadata to
in-process providers without opening sockets, while legacy fixture refs and
future protocol refs still require explicit materializers.
Those targets can now be materialized into existing scheduler retention-provider
handles from caller-supplied in-process fixture daemons only when the provider
id is present exactly once and the fixture network id matches. Missing,
duplicate, or wrong-network providers are reported instead of being used.
The scheduler also has a local-only stored-provider run path that accepts those
in-process fixture daemon refs directly, materializes selected fixture handles,
and then runs the existing stored-provider quorum and retention logic. That
keeps test runs socketless while exercising the same scheduler behavior as
future protocol materializers. The membership-log scheduler has the same
fixture-daemon stored-provider path, so local tests cover pulling/publishing
membership logs plus retained settings objects without binding loopback ports.
Stored-provider runtime ticks exclude unsupported endpoint refs from
materialized provider quorum even when the caller supplies a daemon handle with
the same unsupported string, so socket-shaped metadata cannot be laundered into
an accepted fixture provider. The
membership-aware scheduler has the same stored-provider path: its
read-only plan preserves the no-mutation boundary and does not pull membership
records to satisfy credential preflight, while its runtime path pulls the
membership log first and then selects stored retention providers from the
updated `slate-settings.db` view. The
selected-handle runtime path also rejects a provider set that cannot meet the
requested retaining-provider quorum before publishing local objects, pulling
candidates, retaining objects, or mutating sync roots. If a selected fixture
provider refuses retention because of local quota or pinning policy, the cycle
surfaces that as a retention error instead of reporting successful durability.
Cadence, platform key-store loading, enrollment flow, and real provider daemon
construction remain separate runtime layers.
Object bytes are also provider-held: fetches require at least one online
provider with the object, and retaining an object copies the bytes into the
retaining provider's in-process store. Tests can pause object transfer from one
simulated device provider to another; while paused, the target treats the
source-held encrypted bytes as unavailable without sleeping, binding sockets, or
contacting any external network. Tests can also pause mutable-root propagation
from one simulated publishing device to another independently from object
transfer, so root freshness and encrypted-object availability can fail in
separate ways. Root health reports delayed mutable-root candidate counts,
delayed publisher provider ids, and delayed object-transfer provider ids, so
scheduler-facing health can distinguish a truly empty root, a root hidden by
delayed in-process propagation, and a visible root whose latest object is
temporarily unreachable. The fixture keeps one visible root candidate per
publishing device and can list competing candidates in newest-first order,
giving merge tests a local model for equal-control devices publishing different
signed roots.
The fixture also models availability-only providers: they may retain and serve
encrypted bytes, but their provider policy denies mutable-root publishing and
discovery reports that boundary.

## Privacy Boundaries

- Sync must be opt-in per profile.
- Sync writes must never use public gateways unless an explicit backend policy
  says the encrypted object may be written there.
- Public gateway reads are not acceptable for encrypted profile sync unless the
  user explicitly configures that policy.
- Kubo RPC must default to loopback-only endpoints when used.
- DHT providing, reproviding, and remote pinning require visible user settings.
- Relays and discovery services may learn device identifiers, timing, and
  traffic volume. Slate must surface that policy when a backend uses them.
- The browser should show when sync is degraded because no logged-in device is
  currently pinning the newest root.
- Contracted or self-hosted providers may retain and serve encrypted objects but
  must not receive device signing keys, recovery keys, or plaintext profile
  data.
- No profile data leaves the machine before encryption and signing.

## Implementation Slices

1. Add local syncable `slate-settings.db` migrations for values, changes,
   snapshots, revisions, device state, and app sync domains.
2. Make local settings writes produce typed `sync_change` records and update the
   materialized settings view in one transaction.
3. Add a local runtime watcher that observes revisions and applies typed changes
   through normal browser-core, chrome, routing, and privacy paths.
4. Add the protocol-neutral `profile-sync` service trait in broadwebd with a
   fake in-memory backend and policy checks.
5. Add backend roles for discovery, connectivity, transfer, availability, and
   mutable roots.
6. Add the equal-control account authority model: device heads, membership
   epochs, enrollment records, and provider records with no write authority.
7. Add Kubo RPC helpers for add, pin, unpin, pin verification, IPNS publish, and
   IPNS resolve, restricted to local endpoints by default.
8. Implement signed encrypted manifest, device-head, snapshot, and change
   encoding.
9. Add the `slate-profile-sync` runtime bridge that lets storage pull through
   broadwebd without depending on protocol internals.
10. Wire local publish, transfer, and retain flows from storage into broadwebd.
11. Add mutable-root load and merge flow for a second local profile instance.
12. Evaluate Iroh as an online trusted-device transfer and discovery backend.
13. Implement retention and snapshot compaction heuristics.
14. Add device enrollment, revocation, and key rotation.
15. Add ignored/manual backend integration tests and leak tests that verify sync
    does not use OS DNS, public gateways, relays, or discovery services outside
    the selected policy.
16. Generalize the service contract so future transports can back profile sync.

## Testing Strategy

- Pure unit tests cover schema migration, merge policy, conflict handling,
  retention, and compaction.
- Fake broadwebd tests cover publish, retain, resolve, discovery, transfer, and
  failure handling without external network access.
- Local distributed-protocol fixtures model peer discovery, mutable records,
  object transfer, pinning or availability, offline devices, delayed sync, and
  conflicts inside the test process, without loopback sockets, the real
  internet, Tor, public IPFS/IPNS, or any external relay.
- Downstream profile-sync runtime tests should construct broadwebd registries
  through `InProcessBroadwebNetwork` so simulated devices share one in-memory
  provider graph and fixture HTTP fetches reject non-synthetic URLs before DNS
  or socket access.
- Fixture broadwebd health should advertise `socketless-fixture` for internal
  HTTP, IPFS gateway, and Kubo RPC transports. Synthetic Kubo fixture fetches
  must resolve through the in-process registry before constructing a real HTTP
  client, so default tests never depend on loopback listeners or firewall state.
- App-owned sync watchers should persist profile/domain-scoped cursors in
  `slate-settings.db` after applying a batch, and cursor advancement should be
  monotonic so duplicate or stale fixture deliveries cannot rewind app state.
  The storage cursor-backed poll helper initializes missing cursors at the
  domain head, then leaves advancement to the caller after app-owned state is
  updated. The typed watcher apply helper should call app-owned apply code
  before acknowledging a batch and keep the previous cursor if apply fails.
- Kubo integration tests are ignored or environment-gated and run against
  loopback only.
- Leak tests assert that sync never falls through to DNS, public gateways,
  relays, or discovery services outside the selected policy.
- UI tests cover enabling sync, degraded sync state, and live application of
  externally synced settings.

## Progress Discipline

Work should move in small reviewable steps. Each coherent documentation,
schema, service, fixture, or wiring change should be committed separately so
progress is recoverable and easy to bisect. After each step, rerun the smallest
relevant test set with memory-constrained build settings, then periodically
revalidate previously touched storage, broadwebd, rail app, and chrome
behaviors to catch regressions before larger integration work accumulates.

## References

- Kubo RPC API: https://docs.ipfs.tech/reference/kubo/rpc/
- IPNS concepts: https://docs.ipfs.tech/concepts/ipns/
- IPFS persistence and pinning: https://docs.ipfs.tech/concepts/persistence/
