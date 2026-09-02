{
  description = "jojobot — a personal-assistant server exposed through one MCP";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    # **The store's engine tracks unstable, the rest of the toolchain does
    # not.** dolt moves faster than a release channel does, and a deploying
    # host runs the version it tracks rather than the one this flake pinned —
    # so the version the tests run against is the newer one, not the older.
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
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
      nixpkgs-unstable,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # The one package taken from unstable — see the input's note.
        dolt = (import nixpkgs-unstable { inherit system; }).dolt;

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
          # The store's tests spawn a real `dolt`, so the check phase needs the
          # binary the same way the dev shell does — a package build that ran
          # the suite without it failed every one of those tests, which is the
          # build saying the toolchain is short rather than the code being
          # wrong.
          # **The zone database, for the same reason.** jojobot resolves IANA
          # zone names — a run says which zone it works in — and jiff reads
          # them from a directory on disk. A deployed host has one at
          # `/etc/zoneinfo`; the build sandbox has nothing at any of the paths
          # jiff looks in, so the suite that reads a real zone fails there and
          # nowhere else. `TZDIR` names the store path directly, which is the
          # one lookup that does not depend on the sandbox's filesystem.
          nativeCheckInputs = [
            dolt
            pkgs.tzdata
            pkgs.python3
          ];
          TZDIR = "${pkgs.tzdata}/share/zoneinfo";
          # scripts/sabotage carries `#!/usr/bin/env python3`. The check
          # phase spawns it straight from the source tree, before fixup's
          # patchShebangs ever runs — and that only rewrites $out anyway,
          # never the source. The sandbox has no /usr/bin/env, so the tests
          # that spawn it get ENOENT unless the shebang is patched first.
          preCheck = ''
            patchShebangs scripts
          '';
          # **Two kinds of test live in this workspace, and only one of them
          # is about the package being built here.** Most suites prove the
          # shipped binary behaves; a few prove something about the DEV-TIME
          # tooling around it instead, and neither belongs in a packaged
          # build's checkPhase — they test a tool or a workflow, not the
          # artifact this derivation produces.
          #
          # `jojobot-exercise` is the whole crate for one of those: its own
          # manifest calls it "the third test tier", a harness that drives a
          # real model through a throwaway instance and costs money to run
          # for real. Nothing in the shipped `jojobot` binary depends on it.
          # Its free unit tests prove the harness itself works — spawning a
          # real jojobot+dolt process pair and watching for a ready line —
          # which `make check` already does, in the dev shell that harness
          # was built for. Excluded at the crate level rather than skipped
          # test by test, because the whole crate is the same kind of thing.
          #
          # fixture_roster's unpushed-commit check is the other kind, at test
          # granularity: it reads `git log origin/main..HEAD` against the
          # checkout it runs in, and fails loud rather than passing quietly
          # when it cannot — by design, so a missing check is never reported
          # as a clean one. `src` here is a Nix store path: it has no `.git`,
          # the same reason JOJOBOT_BUILD below falls back to "unknown"
          # instead of reading `self.rev`. That makes the check unrunnable in
          # this checkPhase on every build, not occasionally broken.
          #
          # Both stay enforced by `make check`, which is where dev-time
          # tooling is actually exercised.
          cargoTestFlags = [
            "--workspace"
            "--exclude=jojobot-exercise"
          ];
          checkFlags = [
            "--skip=no_unpushed_commit_message_writes_a_pronoun_for_the_operator"
          ];
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
            dolt
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
