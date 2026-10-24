# Scoped synchronization and continuity

`lucy peer list` shows enrollments and the local public signing key.
`lucy peer enroll --stdin` accepts a versioned peer configuration (example in
`peers.json`). Enrollment explicitly sets scope, public/private status, upload
permission, trusted actor keys, local source files and the credential environment
variable. Never put a token in this document or in memory. Read-only public
registry entries cannot authorize private access or upload.

`lucy sync` reads remote revisions through GitHub's HTTPS API. Automatic uploads
write only `lucy-memory`, never a code branch. Ref updates have `force:false`;
a race triggers fetch/union/validation and at most three attempts. Pending
operations remain durable locally. Failures retain the last good cache.

Each canonical Nibli `record(..., Operation, Json).` contains a signed version 1
operation: actor/counter ID, parents, scope, target, change, exact payload,
provenance and Ed25519 signature. The actor public key must already be enrolled.
An operation ID cannot change content. Addition identities are immutable.
Corrections, retractions and explicit restorations are new operations. Concurrent
corrections remain conflicts. Tombstones prevent stale replay. No wall clock
selects a winner. Git history can retain deleted content; deletion is not erasure.

Use `lucy peer change --stdin` with `{ "peer": "NAME", "target": "ID",
"change": "correct", "payload": {…}, "provenance": {…} }` for reviewed changes.
`retract` takes a null payload; `restore` must explicitly follow a tombstone.
Exact-origin routes forward corrections, tombstones and restorations as well as
additions, retaining the original per-record causality across scope translation.
Snapshots are deterministic Nibli. Conflicting assertions block their reasoning
projection; original evidence stays available. redb and embeddings are caches.

Legacy imports retain exact source bytes and checkpoint only validated archives.
Invalid input is quarantined privately and reported on every retry. It is never
silently repaired or published as a successful import. Old JSON originals remain
available during migration; an unclassified private record stays local.

`lucy continuity since --consumer ID --scope S` reads durable observations and
returns an opaque cursor. Reading does not acknowledge. `lucy continuity ack
--consumer ID --scope S --cursor C` persists position idempotently. Cursors bind
to the store, consumer and scope. The session-bound RPC/MCP interface derives
its consumer from the activated source/session. The service refreshes peers every
15 minutes (configurable), with no automatic model inference.

Before replacing a runtime, make a private `lucy store backup`. Keep the previous
binary/configuration for rollback and retain the new `replication-v1` operation
logs even if an older runtime cannot interpret them. Do not commit a live database
or signing key. NixOS staging and reboot are separate operator actions.
