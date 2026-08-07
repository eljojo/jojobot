{
  description = "jojobot — a personal-assistant server exposed through one MCP";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Toolchain comes from rust-toolchain.toml so dev and CI agree.
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.openssl ];
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "jojobot";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          inherit nativeBuildInputs buildInputs;
          # What `ping` reports as the running build. This has to come from
          # here: the build sandbox has no `.git` — src is a store path — so
          # the build script's git fallback cannot fire, and the deployed
          # binary is exactly the one nobody can identify from outside.
          # A dirty tree has no `rev`, hence the fallbacks; `unknown` is a
          # real answer and the build script treats it as one.
          JOJOBOT_BUILD = self.rev or self.dirtyRev or "unknown";
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          # gnumake so `nix develop -c make check` works on a machine that has
          # no make of its own — the Makefile is the green bar written down, and
          # a runner you have to install separately is one people skip.
          buildInputs = buildInputs ++ [
            rustToolchain
            pkgs.gnumake
            # jojobot spawns `dolt sql-server` and supervises it, so the binary
            # is part of the toolchain rather than a service somebody installs:
            # the mailbox and session tests start a real one against a temp
            # directory, and a test that skips when a binary is missing is a
            # test nobody notices stopped running.
            pkgs.dolt
          ];
          # Cargo's default ./target (gitignored) — no CARGO_TARGET_DIR override,
          # which would anchor to the shell-entry $PWD and leak artifacts if run
          # from a parent directory.
        };
      }
    )
    // {
      # System-agnostic outputs, for consumers deploying jojobot.
      overlays.default = final: prev: {
        jojobot = self.packages.${prev.stdenv.hostPlatform.system}.default;
      };
      nixosModules.default = import ./nix/modules/jojobot.nix;
      nixosModules.jojobot = import ./nix/modules/jojobot.nix;
    };
}
