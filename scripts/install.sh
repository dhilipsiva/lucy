#!/bin/sh
set -eu
# CLI installation only. No service, adapter enrollment or model download.
version=${LUCY_VERSION:-v0.3.1}
bin_dir=${LUCY_BIN_DIR:-"$HOME/.local/bin"}
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) target=x86_64-unknown-linux-musl ;;
  Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-musl ;;
  Darwin/x86_64) target=x86_64-apple-darwin ;;
  Darwin/arm64) target=aarch64-apple-darwin ;;
  *) printf '%s\n' 'Unsupported platform; Windows uses install.ps1.' >&2; exit 1 ;;
esac
case "$version" in v[0-9]*.[0-9]*.[0-9]*) ;; *) printf '%s\n' 'Invalid release version' >&2; exit 1;; esac
command -v curl >/dev/null
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
asset="lucy-$version-$target.tar.gz"
base="https://github.com/dhilipsiva/lucy/releases/download/$version"
curl --fail --location --proto '=https' --tlsv1.2 "$base/$asset" -o "$work/$asset"
curl --fail --location --proto '=https' --tlsv1.2 "$base/SHA256SUMS" -o "$work/SHA256SUMS"
expected=$(awk -v name="$asset" '$2 == name { print $1 }' "$work/SHA256SUMS")
test ${#expected} -eq 64
if command -v sha256sum >/dev/null; then actual=$(sha256sum "$work/$asset" | cut -d' ' -f1)
else actual=$(shasum -a 256 "$work/$asset" | cut -d' ' -f1); fi
test "$actual" = "$expected" || { printf '%s\n' 'Checksum mismatch' >&2; exit 1; }
# Extract the one named executable, not paths supplied by the archive.
tar -xzOf "$work/$asset" lucy > "$work/lucy"
chmod 755 "$work/lucy"
"$work/lucy" --version
mkdir -p "$bin_dir"
if test -e "$bin_dir/lucy"; then cp -p "$bin_dir/lucy" "$bin_dir/lucy.previous"; fi
install -m 755 "$work/lucy" "$bin_dir/lucy.new"
mv -f "$bin_dir/lucy.new" "$bin_dir/lucy"
printf 'Installed %s. Add %s to PATH if needed.\n' "$bin_dir/lucy" "$bin_dir"
printf '%s\n' 'Service, adapters and model installation are separate choices; see INSTALL.md.'
