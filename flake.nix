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
    # Builds the dependency graph once, apart from the workspace — see
    # `cargoArtifacts` below.
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      rust-overlay,
      flake-utils,
      crane,
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

        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.openssl ];

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # **What the package build reads, and nothing else.** The workspace
        # sources with every file beside them (migrations, fixtures and
        # served text are read at compile time), the manifests and the
        # toolchain pin. A change to a document, a card or the flake's own
        # prose is outside this set, so it does not change the package's
        # source and nothing recompiles.
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./rust-toolchain.toml
            ./crates
          ];
        };

        # **The checks read more than the package does.** Some suites read the
        # repository's own documents and scripts, so the checks take the whole
        # tree. They build on `testArtifacts`, so the dependencies are still
        # built once.
        checkSrc = ./.;

        # What `ping` reports as the running build. This has to come from
        # here: the build sandbox has no `.git` — src is a store path — so
        # the build script's git fallback cannot fire, and the deployed
        # binary is exactly the one nobody can identify from outside.
        # A dirty tree has no `rev`, hence the fallbacks; `unknown` is a
        # real answer and the build script treats it as one.
        # **Set on the package only, never on the dependencies:** it changes
        # on every commit, and a dependency build that read it would rebuild
        # every commit.
        build = self.rev or self.dirtyRev or "unknown";

        common = {
          pname = "jojobot";
          version = "0.1.0";
          strictDeps = true;
          inherit nativeBuildInputs buildInputs;
        };

        # **Every crates.io dependency, built once.** crane builds the
        # dependency graph from the manifests and the lock file alone, so the
        # result is keyed by those and by nothing a commit usually touches.
        # The package builds on it.
        cargoArtifacts = craneLib.buildDepsOnly (
          common
          // {
            inherit src;
            cargoExtraArgs = "--workspace --locked";
          }
        );

        # **The suite's dependencies are a second build, on purpose.** The
        # suite compiles dev-dependencies, which switch on features of the
        # shared dependencies. Cargo builds a crate once per feature set, so
        # one build serving both would leave the package recompiling
        # whichever half it did not match. `--all-targets` compiles the
        # test executables' dependencies for real.
        testArtifacts = craneLib.buildDepsOnly (
          common
          // {
            inherit src;
            pname = "jojobot-tests";
            cargoExtraArgs = "--workspace --exclude=jojobot-exercise --locked";
            cargoBuildCommand = "cargo build --profile release --all-targets";
          }
        );

        # The shipped package. It does not run the suite: `checks.tests`
        # below runs it against the same sources, so `nix flake check` still
        # tests the artifact that deploys.
        jojobot = craneLib.buildPackage (
          common
          // {
            inherit src cargoArtifacts;
            cargoExtraArgs = "--workspace --locked";
            doCheck = false;
            JOJOBOT_BUILD = build;
          }
        );

        # **The suite, moved out of the package build and kept whole.**
        # The same exclusions and the same sandbox environment the package's
        # check phase had.
        tests = craneLib.cargoTest (
          common
          // {
            src = checkSrc;
            cargoArtifacts = testArtifacts;
            # The store's tests spawn a real `dolt`, so the check needs the
            # binary the same way the dev shell does — a build that ran
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
              # The `bar` runner's size-report tests build a scratch repository
              # of their own, and the sandbox has no `git`.
              pkgs.git
            ];
            TZDIR = "${pkgs.tzdata}/share/zoneinfo";
            JOJOBOT_BUILD = build;
            # scripts/sabotage carries `#!/usr/bin/env python3`. The check
            # spawns it straight from the source tree, and the sandbox has no
            # /usr/bin/env, so the tests that spawn it get ENOENT unless the
            # shebang is patched first.
            preCheck = ''
              patchShebangs scripts
            '';
            # **Two kinds of test live in this workspace, and only one of them
            # is about the package being built here.** Most suites prove the
            # shipped binary behaves; a few prove something about the DEV-TIME
            # tooling around it instead, and neither belongs in this check —
            # they test a tool or a workflow, not the artifact.
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
            # the same reason JOJOBOT_BUILD above falls back to "unknown"
            # instead of reading `self.rev`. That makes the check unrunnable in
            # this sandbox on every build, not occasionally broken. The two
            # checks that read the commit range from the roster boundary to HEAD
            # are the same kind, for the same reason.
            #
            # Both stay enforced by `make check`, which is where dev-time
            # tooling is actually exercised.
            cargoTestExtraArgs = builtins.concatStringsSep " " [
              "--workspace"
              "--exclude=jojobot-exercise"
              "--"
              "--skip=no_unpushed_commit_message_writes_a_pronoun_for_the_operator"
              "--skip=no_commit_message_since_the_boundary_quotes_an_off_roster_handle"
              "--skip=the_handle_check_range_reaches_head"
            ];
          }
        );
      in
      {
        packages.default = jojobot;
        checks = { inherit tests; };

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
