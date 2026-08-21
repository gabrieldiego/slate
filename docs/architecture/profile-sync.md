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
- The manifest identifies the current snapshot and unsquashed tail changes.
- The manifest is signed by an authorized device key or by an account policy
  threshold.
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

The active-key pull path also exposes an idempotent root-status helper for sync
polling. It resolves the published root first and reports missing roots,
unchanged already-verified roots, or applied manifests separately. If the
published root object id matches the locally stored verified root, storage does
not fetch, decrypt, or apply the object set again.

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
  membership_epoch
  latest_manifest_object_id
  latest_change_object_id
  device_sequence
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

The first synced domains should be settings that are safe to apply live, such as
UI preferences, protocol adapter configuration, rail app ordering, and
bookmarks. More sensitive domains need separate threat notes before entering
sync.

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
  sees it.
- `contacts`: contact cards, identities, groups, local aliases, and provider
  mapping. Contacts are sensitive and should not be synced before the app has a
  dedicated privacy note.
- `chat`: account/provider configuration, local conversation metadata, and
  aggregation preferences. Message contents, SMS/WhatsApp secrets, and provider
  tokens require separate designs before sync.
- `files`: file metadata, user-selected sync sets, directory manifests, content
  object references, retention policy, and local availability state. File bytes
  may use heavier transfer/storage backends than settings changes.
- `downloads`: download history, source routing metadata, integrity metadata,
  and user-selected persistent file records. Temporary downloads should stay
  local unless explicitly promoted.
- `storage`: broadweb storage providers, pinning leases, quotas, object health,
  and repair metadata.
- Future apps such as Player, Notes, Tasks, Mail, or Media Library should define
  their own sync domain before storing replicated state.

No app should bypass `profile-sync` with an ad hoc network path for replicated
profile state. If an app needs a protocol-specific backend, broadwebd should
expose it through the same discovery, connectivity, transfer, availability, and
mutable-root roles.

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
- Discover approved devices and providers that may have profile sync objects.
- Establish direct, relayed, local, or private-network transfer sessions.
- Report backend health and publish failures.
- Stream sync availability events to Slate.

The first implementation should use a fake backend in unit tests. Kubo RPC can
be the first IPFS/IPNS backend for manual or ignored integration tests. Iroh,
Syncthing-like device providers, Tahoe-LAFS-style storage, Tor-reachable home
daemons, and non-IPFS backends should fit behind the same application service
contract when they prove useful.

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
9. Wire local publish, transfer, and retain flows from storage into broadwebd.
10. Add mutable-root load and merge flow for a second local profile instance.
11. Evaluate Iroh as an online trusted-device transfer and discovery backend.
12. Implement retention and snapshot compaction heuristics.
13. Add device enrollment, revocation, and key rotation.
14. Add ignored/manual backend integration tests and leak tests that verify sync
    does not use OS DNS, public gateways, relays, or discovery services outside
    the selected policy.
15. Generalize the service contract so future transports can back profile sync.

## Testing Strategy

- Pure unit tests cover schema migration, merge policy, conflict handling,
  retention, and compaction.
- Fake broadwebd tests cover publish, retain, resolve, discovery, transfer, and
  failure handling without external network access.
- Local distributed-protocol fixtures model peer discovery, mutable records,
  object transfer, pinning or availability, offline devices, delayed sync, and
  conflicts inside the test process, without loopback sockets, the real
  internet, Tor, public IPFS/IPNS, or any external relay.
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
