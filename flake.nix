{
  description = "Dev environment for tracing-systemd";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        # rust-toolchain.toml is the single source of truth for channel and
        # components, so the shell's cargo and rust-analyzer cannot drift from it.
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          name = "tracing-systemd";
          packages = [
            toolchain
            pkgs.rust-analyzer
            pkgs.pkg-config
            pkgs.systemd # journald headers for the `journald` feature and example
            pkgs.cargo-nextest
            pkgs.cargo-llvm-cov
          ];
          # The shell's rust-src (a `rust-src` component above) is what hover on
          # core/std needs; the profile rust-analyzer is a different toolchain and
          # resolves std against nothing without this.
          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
          RUST_BACKTRACE = "1";
        };
      });
}
