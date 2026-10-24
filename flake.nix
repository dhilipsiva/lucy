{
  description = "Lucy: independent memory and agent runtime";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = { url = "github:oxalica/rust-overlay"; inputs.nixpkgs.follows = "nixpkgs"; };
  };
  outputs = { nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; };
          rust = pkgs.rust-bin.stable."1.96.1".default.override { extensions = [ "clippy" "rustfmt" ]; };
      in {
        devShells.default = pkgs.mkShell { packages = [ rust pkgs.just pkgs.python3 pkgs.pkg-config ]; };
        devShells.desktop = pkgs.mkShell {
          packages = [ rust pkgs.just pkgs.python3 pkgs.pkg-config pkgs.gtk4 ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.xvfb-run pkgs.xdotool pkgs.dbus ];
        };
      });
}
