# Profile Storage

Slate-owned user state lives in a local SQLite file named `slate-settings.db`.
The filename is intentionally product-oriented instead of database-oriented so
Slate can change implementation details later without changing the user-facing
concept.

The database stores Slate-owned:

- settings
- bookmarks
- cookies
- browsing history
- small binary blobs such as favicons, thumbnails, or serialized app state

## Path Resolution

At startup, Slate resolves the database path in this order:

1. Use the explicit CLI path from `--settings-db <path>`.
2. If no path was specified, use `./slate-settings.db` from the launch working
   directory when it already exists.
3. If no launch-directory file exists, use `~/.slate/slate-settings.db` when it
   already exists.
4. If neither implicit file exists, create a new `./slate-settings.db` in the
   launch working directory with default schema and settings.

This means a local project or portable folder can carry its own Slate state by
placing `slate-settings.db` next to the command invocation. Existing
`~/.slate/slate-settings.db` remains a fallback for users who want a home-level
profile without passing a flag.

## SQLite Choice

SQLite is used because it is a stable single-file embedded database with good
support for structured browser data, indexes, transactions, and `BLOB` values.
Slate accesses it through `rusqlite` from safe Slate-owned Rust. The unsafe FFI
boundary remains inside the mature SQLite binding dependency, not in Slate-owned
code.

For now Slate uses SQLite rollback journal mode instead of WAL mode so the
steady-state profile is one main database file. SQLite may create temporary
journal files during writes.

## Schema Direction

The initial schema creates these tables:

- `settings`: key/value Slate settings.
- `bookmarks`: profile-scoped bookmark metadata with optional favicon blob keys.
- `cookies`: profile-scoped cookie records.
- `browsing_history`: profile-scoped visit records and visit counts.
- `binary_blobs`: profile-scoped binary values keyed by caller-defined names.

The storage crate exposes APIs for reading and writing these records so
Slate-owned features can keep their state in this file instead of adding new
sidecar files.

The chrome zoom setting and configurable browser key bindings are persisted
through `settings`. The first set covers tab navigation plus editing actions
such as cut, copy, paste, and select all. The internal settings page previews
slider changes in memory immediately, but writes the selected zoom and shortcut
values to `slate-settings.db` only when the user activates Save. Browsing history is
recorded when Servo reports history changes.

Settings should fail independently. If a stored setting is missing, malformed,
or temporarily unreadable during development, the caller should use that
setting's default value without clearing unrelated settings, bookmarks,
history, cookies, or blobs.

When Slate creates a new profile database, or opens an older empty profile that
has not recorded default bookmark seeding yet, it seeds first-run bookmarks for
Wikipedia on IPFS and OpenStreetMap in the default profile. The home page fills
remaining visible bookmark slots with non-persistent placeholders that encourage
users to add their own sites. Once the seed marker is written, deleting those
bookmarks does not make Slate recreate them on the next launch.

The address-bar bookmark button saves the active page into the first two home
bookmark slots only. User-added bookmarks replace the seeded suggestions first;
after both visible slots are user-owned, adding another bookmark updates an
existing matching slot or replaces the second slot.

Home bookmark favicons are cached in `binary_blobs` using deterministic keys
based on the root favicon URL when one can be derived, falling back to the
bookmark URL otherwise. While an icon is absent, being fetched, failed, or
unsupported by the current raster image decoder, the home card shows a muted
Slate icon. Favicon fetches use the same broadwebd route as subresource loads
and are only started for the active home view.

## Syncable State Direction

The settings database should become a local materialized view over typed profile
state, not the raw object replicated between devices. Broadweb profile sync
should publish signed and encrypted manifests, snapshots, and changes through
approved backends, then let each device apply validated updates into its local
SQLite database.

Planned sync tables:

- `settings_values`: current local values by settings domain and key.
- `settings_changes`: append-only local and remote change records.
- `settings_snapshots`: compacted sync snapshots and their backend object ids.
- `settings_revisions`: monotonic revision rows for runtime watchers.
- `sync_state`: device frontiers, mutable roots, publish state, and retention
  metadata. Mutable-root pulls can enumerate competing backend candidates and
  verify each signed manifest before storage records an applied root.
- `app_sync_domains`: registered first-party app domains, schema versions,
  enabled/paused state, and privacy classification.

Runtime code should observe `settings_revisions` or an equivalent typed event
stream. It should not watch raw WAL pages or accept direct replacement of the
database file as a live update mechanism. Applying synced changes should go
through the same chrome, routing, privacy, and protocol configuration paths as
local settings edits.

The first synced domains should be low-risk and useful:

- UI preferences.
- Rail app order and enabled app list.
- Bookmarks.
- Protocol adapter configuration that has no embedded secret.

The minimum implementation should start here, without requiring IPFS, Iroh,
Kubo, relays, or any other network backend. Local writes should create typed
change records, bump a revision, update the materialized values, and be
observable by runtime code. Once that works, broadwebd can publish the same
typed changes and snapshots through selected broadweb backends.

First-party apps should register their sync domains before syncing replicated
state:

- `calendar`: events, reminders, recurrence metadata, and provider mapping.
- `contacts`: contact cards, identities, groups, local aliases, and provider
  mapping.
- `chat`: provider configuration, local conversation metadata, and aggregation
  preferences.
- `files`: directory manifests, selected sync sets, file metadata, content
  object references, and retention policy.
- `downloads`: persistent download records, source routing metadata, integrity
  metadata, and user-promoted files.
- `storage`: provider configuration, pinning leases, quotas, object health, and
  repair metadata.

Do not sync cookies, passwords, private browsing state, site storage, bearer
tokens, or protocol private keys until separate privacy and key-storage designs
exist for those domains.

Profile sync uses broadwebd's `profile-sync` service for encrypted object
transfer, retention, provider discovery, and mutable-root publishing. Logged-in
devices should retain the current root and recent tail objects by default.
Remote pinning, provider behavior, relays, discovery services, and public
gateway reads require explicit user policy.

Compaction should be ack-aware with a timeout. Keep deltas needed by recently
seen devices and all changes newer than the retention window, then squash older
state into an encrypted snapshot. Devices that miss the window rejoin from the
newest snapshot and rebase any local unsynced changes.

## Integration Notes

Bookmark editing should be wired through Slate's bookmark UI once that UI
becomes functional.

Servo still owns the active HTTP cookie jar today and persists it through its
resource-thread configuration path as `cookie_jar.json`. Moving live HTTP
cookies into `slate-settings.db` requires a later Servo/network integration
point so cookie reads, writes, expiry, and clearing flow through Slate storage
without duplicating state.
