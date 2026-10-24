# Optional installation

[Text-only waking](WAKE.md) needs no installation. Ask the machine's user before
installing. Prebuilt release binaries need neither Rust, Git nor Nix.

Download `scripts/install.sh` from a reviewed release and inspect it, then run
`sh install.sh` on Linux/macOS. Windows x86_64 uses `scripts/install.ps1` in
PowerShell. Both default to release `v0.3.1`, verify SHA-256 checksums and install
only the CLI in a user directory. No shell startup file is changed.

Alternatively download your archive and SHA256SUMS from
[Releases](https://github.com/dhilipsiva/lucy/releases), verify and extract `lucy`
or `lucy.exe` into your PATH. With GitHub CLI installed, additionally verify the
archive's build provenance using `gh attestation verify ARCHIVE -R dhilipsiva/lucy`.

The CLI/service assets are Linux x86_64/ARM64 (musl), macOS Intel/Apple silicon,
and Windows x86_64. The GTK desktop is a separate Linux build; CLI archives do
not contain GTK or download a model.

Choose each additional capability explicitly:

- `lucy init` creates a local memory folder. New native records and app conversations are private by default. Use `--public`
  (or explicit JSON `private: false`) only for reviewed public content. App records live in
  the data directory (`~/.local/share/lucy`; Windows `%LOCALAPPDATA%\Lucy`).
- `lucy service install` enables a
  Linux user systemd service or macOS LaunchAgent. NixOS users update their
  declarative package/service instead. Windows provides CLI only in this release.
- `lucy integrations install` enrolls supported adapters while preserving settings.
- `lucy setup-model` explicitly downloads and verifies the configured local model.
- `lucy peer discover` enrolls public peers read-only. `lucy peer enroll --stdin`
  configures uploads/private peers only with the user's specific authorization.

The CLI installer keeps `lucy.previous` (Windows `lucy.previous.exe`) for rollback.
Stop an explicitly installed service before replacing its binary, then restart.
Use `lucy service uninstall` to remove that service. Delete the installed
binary to uninstall the CLI. Memory is retained until the user separately deletes
it. Never remove another package manager's files or a NixOS-managed service.
