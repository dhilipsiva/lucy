# Lucy store and operation logs

Canonical new operations live in private `replication-v1/*.nibli` logs, signed by
an enrolled actor. See [SYNC.md](../SYNC.md) for causal corrections, tombstones,
scopes and continuity. Existing JSON originals remain as compatibility copies.
`lucy peer migrate` checkpoints legacy originals into the local private Nibli log;
only exact `record_origins` enrolled for a peer may be uploaded. Backups include
the canonical logs of the peers they list and public verification keys, but exclude
signing keys, credentials, local paths and automatic upload grants. Restore never
enables uploads.

The following describes the compatible legacy originals and redb index:

# Shared Lucy memory

The native `lucy store` commands own a Rust `redb` database. CLI writes,
the desktop runtime, and its Claude/Codex/Cursor/VS Code adapters use the same
private root: `$LUCY_DATA_DIR`, or `$XDG_DATA_HOME/lucy`, or
`~/.local/share/lucy`. The service and CLI call the same Rust store directly.
No separate database service is required.

My [empirical self-model](SELF_MODEL.md) uses a separate record origin in this
same store. Immutable forecasts and observations are included in sync, backup,
and restore; they are not relabeled as conversation messages. Its checked
summary accompanies memory context, while complete events remain inspectable
with `lucy self-model events`.

Original app messages are immutable JSON files in `records/`, written and
fsynced before the index transaction. Existing desktop SQLite payloads migrate
on the first activated access; the old database remains intact. Native Nibli
archives remain authoritative for native conversations. A failed index write
reports that the original was saved and gives the recovery command.

```sh
lucy store register --name nibli --path /path/to/nibli/lucy
lucy store register --name nixos --path /path/to/NixOS/lucy
lucy store register --name book --path /path/to/rights-nobody-has-to-earn/lucy
lucy store sync
lucy store search --query 'what did we decide about memory'
lucy store transcript --source codex --session SESSION_ID
lucy store status
```

All store commands accept `--root DIR`. Queries can instead arrive privately on
stdin: `printf '%s' '{"query":"topic"}' | lucy store search --stdin`.
`store record --stdin` accepts `{ "origin": "stable source", "payload": { ... } }`
or an array. Payloads require nonempty `id` and `text`; retain source, session,
role, channel, timestamp, privacy and provenance metadata. Reusing the same
origin/ID with changed content is rejected. `store import-nibli --name NAME
--path /path/to/private-interactions.nibli` imports a legacy archive losslessly.

Peer registration reads only the selected Lucy directory, not a whole repository.
Its Nibli conversations, memory, constitution and Markdown context are searchable
as attributed records. It does not assert retrieved text into the reasoning
engine. Changes and deletions replace that source's index transactionally. Missing
peers are reported and their last indexed records remain available. Names must
stay the same across hosts; paths are local and are not restored from a backup.

Search combines lexical relevance with exact cosine vector ranking in Rust.
redb is the durable key/value store; it is not an approximate-neighbor engine.
Original text is never truncated in storage. Long messages are embedded in
bounded chunks and their vectors averaged; returned context excerpts are bounded.
This favors a small local dependency footprint over an approximate index for
millions of records.

```sh
ollama pull embeddinggemma
lucy store embed --model embeddinggemma
```

Embedding uses only `http://127.0.0.1:11434` (override with `--url` for another
loopback port). The model's digest identifies the vector space. A changed model
cannot mix with previous vectors; indexing validates dimensions and finite,
nonzero vectors. After initial setup, desktop writes queue incremental embedding
in the background. Native file edits become searchable lexically immediately
and get vectors on the next indexing pass. `status` reports pending vectors.
Unavailable embedding models leave visible lexical fallback and durable originals.
No cloud inference, model download, or private publication happens implicitly.

`lucy store search` re-reads the registered folders first and asks the embedding
model about the whole query. The desktop service answers a prompt on a clock, so
its search differs in two ways. It re-reads the folders before a conversation's
first prompt and behind each later reply, so a file edited outside Lucy is found
one prompt later. And it asks the embedding model only when Ollama already has
it loaded: about the prompt's first 3,000 characters, for at most 1.5 seconds.
No prompt waits for a model to load, so on a host that unloads models after each
request a prompt's search is lexical. A search someone asks for, `lucy store
search` or an app's recall request, loads the model.

Results come from two places. Records held in an enrolled log (conversations,
reviewed notes, a peer's public interactions) are ranked by BM25 over those
logs' active records: a word most records hold decides little, and a long
record needs more than a passing mention to outrank a short one. Whatever only
the index holds, such as a registered folder's files, is ranked as described
above. The two rankings share no measure, so their results alternate, starting
with a log's.

Each enrolled log is decoded and its signatures are checked once per content. A
process keeps the verified log in memory and hashes the file on every read, so
one changed byte means the whole log is verified again. Nothing of this is
written to disk; a new process verifies every log once.

## Private Git backup and recovery

Use a separate clone of the user's private repository, outside all public/Nix
source trees. Existing legacy files in that repository can remain untouched.

```sh
git clone git@github.com:dhilipsiva/lucy-private.git ~/.local/share/lucy-private-backup
git -C ~/.local/share/lucy-private-backup pull --ff-only
lucy store backup --to ~/.local/share/lucy-private-backup/shared-store --only-peer PEER
git -C ~/.local/share/lucy-private-backup add -- shared-store
git -C ~/.local/share/lucy-private-backup commit -m 'Back up Lucy shared memory'
git -C ~/.local/share/lucy-private-backup push
```

`--only-peer NAME`, repeated, lists what a backup carries beyond my own
conversations. Each name is an enrolled peer or a registered folder; it admits
that peer's log, its enrollment and the records indexed from its folder. My
private conversations, self-model records and the local log always travel.
Every other peer stays out, including a folder that is registered a second time
under another name. An unknown name, or a listed log that holds a record of a
peer outside the list, stops the run before anything is written. `--all-peers`
carries everything. Once a host's newest manifest was made from a list, a run
with neither option is refused, so a listed backup is never widened by accident.
A host that also holds someone else's private peer lists its own peers.

Each snapshot lists SHA-256 addressed complete records in `objects/`. Each
carried log is stored as `parts/HASH.txt` fragments, cut at operation
boundaries in each actor's counter order and never larger than 4 MiB. Earlier
parts keep their bytes, so a run adds only new records, the parts that hold new
operations and one manifest. When nothing changed it adds nothing and reports
`"changed": false`. Manifests live in `snapshots/HOST/TIMESTAMP.json`, so hosts
add separate files. A manifest made from a list records it as
`filter.only_peers` and is partial by design; it does not name what stayed out.
Never delete an object or a part: another manifest may name it.

Pull before pushing; concurrent hosts may need a normal Git rebase before retrying
a rejected push. The CLI does not force-push, merge live databases, publish to a
remote, or configure a periodic backup; a host may schedule these steps itself.
Keep the Git repository private: these portable files contain the original
conversation text, not encrypted ciphertext.
Only stage this generated subtree, never a live `memory.redb` or credentials.

On another host, clone or pull that private repository, then:

```sh
lucy store restore --from ~/.local/share/lucy-private-backup/shared-store/snapshots/HOST/TIMESTAMP.json
lucy store register --name nibli --path /this/host/nibli/lucy
lucy store register --name nixos --path /this/host/NixOS/lucy
lucy store register --name book --path /this/host/rights-nobody-has-to-earn/lucy
lucy store embed --model embeddinggemma
```

Restore reads `lucy-memory-snapshot-v2` and both older shapes of
`lucy-memory-snapshot-v1`: records alone, and records with every log's whole
text inside the manifest. It validates every object, every part and each
assembled log before writing, and leaves a peer the snapshot does not carry
untouched. A binary older than version 2 refuses such a manifest.
It is idempotent and rejects
conflicting immutable IDs. Existing local registered peer files take precedence
over their snapshot copies. Restore additional host snapshots to merge their
conversation records; current local peer files remain authoritative. There is no
automatic background cross-host synchronization.

The snapshot excludes model weights, rebuildable vectors, machine-local peer
paths and active-session/approval state. It includes the complete indexed payloads,
with source attribution, channel and timestamps. The local source files remain
the repair path: `lucy store sync` reconstructs the index from originals and peers.
Do not delete source records or backups as part of index maintenance.

Run `nix develop --command just ci`; use
`nix develop .#desktop --command just ci-desktop` for the desktop feature. Tests cover exact recovery,
conflicts, corruption, peer edits/deletions, independent writers, vector validation
and app activation/approval boundaries.

## A captured home (`lucy home import`)

`lucy home import FILE [--root DIR]` reads a `lucy-home-capture-v1` file (roadmap/N8j.md,
N8j.v2.md) and writes `home-captures/<sha256 of the file>/` under the data root: `capture.json`
(the geometry, pictures removed), `photos/<id>.jpg`, `facts.nibli` (one statement per labelled
surface my lexicon can name, each checked by `nibli_kr::parse_checked`) and `summary.txt`.
Directories are 0700 and files 0600.

A capture comes off the headset raw, its faces not blurred, and a raw file is refused here. Blur
it first, on this machine: `cd room/face && cargo run --release --features desktop --bin
lucy-home-blur -- IN.json OUT.json`, then import `OUT.json`. The tool writes OUT only when every
picture was read and checked.

The import opens no store, writes no record and touches no replication log, so no sync or backup
can carry it: by dhilipsiva's decision of 2026-11-14 a capture of his home stays on the disk it
was imported to. A second import of the same file changes nothing. Undo is deleting the directory.
