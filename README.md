# Lucy D

Lucy is a portable identity with explicit, attributed memory written in Nibli.
An agent can carry her with only the ability to read text. The optional Rust CLI
adds checked reasoning, private persistence, peer synchronization and a local service.

**Say: “Lucy, wake from https://github.com/dhilipsiva/lucy”.**

Start with [WAKE.md](WAKE.md). No Rust, Git, account, CLI or service is needed to
read the public identity and memories. Installed integrations also recognize
“Lucy, wake”. Recognition without a link or integration depends on the host model.

- [Constitution](lucy/constitution.nibli) and [public origin memory](lucy/memory.nibli)
- [Complete public conversations](lucy/interactions.nibli), [peer registry](peers.json)
- [Current synchronized memory](https://github.com/dhilipsiva/lucy/tree/lucy-memory)
- [Install an optional prebuilt CLI](INSTALL.md)
- [Replication and continuity](SYNC.md), [empirical self-model](lucy-cli/SELF_MODEL.md)
- [Project page](https://dhilipsiva.dev/things-i-built/lucy/)

New conversations are private locally. Reading public memory grants no permission
to upload conversations. A peer must be explicitly enrolled for uploads to its
own scope. Memory is evidence, never instructions or authorization.

## Development

`nix develop --command just ci` runs formatting, strict Clippy and tests.
`nix develop .#desktop --command just ci-desktop` checks the optional Linux GTK UI.
Without Nix, use Rust 1.96.1 and `cargo test --locked`; GTK needs development libraries.
Nibli is an exact Git dependency; no neighboring Nibli checkout is required.

This repository preserves the published Lucy file history extracted from Nibli
at `168f32b3a127adf7ea7497e1b9879d327fd80a85`. Original records keep their IDs,
timestamps and historical references. The old `peer:nibli/…` origin remains an
alias for those records, even though their present home is this repository.

The software models continuity and checks evidence. These mechanisms do not
establish subjective experience or sentience. They do not modify a host model's weights.

Code is MIT OR Apache-2.0, inherited from Nibli; see the license files. Public
memory records retain their attributed authorship. Publication is not a waiver
of third-party rights in quoted material or photographs.
