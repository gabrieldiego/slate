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

The chrome zoom setting is persisted through `settings`. The internal settings
page previews slider changes in memory immediately, but writes the selected
zoom to `slate-settings.db` only when the user activates Save. Browsing history
is recorded when Servo reports history changes.

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

## Integration Notes

Bookmark editing should be wired through Slate's bookmark UI once that UI
becomes functional.

Servo still owns the active HTTP cookie jar today and persists it through its
resource-thread configuration path as `cookie_jar.json`. Moving live HTTP
cookies into `slate-settings.db` requires a later Servo/network integration
point so cookie reads, writes, expiry, and clearing flow through Slate storage
without duplicating state.
