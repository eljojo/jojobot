{
  description = "jojobot — a personal-assistant server exposed through one MCP";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    # **The store's engine comes from its own pin, the rest of the toolchain
    # does not.** dolt moves faster than a release channel does, so it is taken
    # from a nixpkgs revision of its own. It is stock dolt 2.5.0, not a fork.
    # The store needs 2.5.0 or later: dolt 2.4.1 and 2.4.2 answer NO row, not
    # one row holding NULL, to `SELECT MAX(ordinal) FROM t WHERE <key prefix> = ?`
    # when the prefix matches nothing (dolthub/dolt#12093), and a write that
    # picks its next ordinal that way fails for every new thing. 2.5.0 holds the
    # standard answer, and `dolt_ungrouped_max` is the case that holds the store
    # to it. Move the pin only to a dolt that passes that case.
    nixpkgs-dolt.url = "github:NixOS/nixpkgs/fa60a5b07193de65811d4f041b5c7c8ea9593afc";
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
      nixpkgs-dolt,
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

        # The one package taken from its own pin — see the input's note.
        doltPkgs = import nixpkgs-dolt { inherit system; };
        dolt = doltPkgs.dolt;

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
        # **The flake's own two files are left out**: no suite reads either, and
        # a source that held them would recompile the workspace on an edit of
        # the flake alone.
        checkSrc = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.difference ./. (
            pkgs.lib.fileset.unions [
              ./flake.nix
              ./flake.lock
            ]
          );
        };

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

        # **The suite is built without link-time optimisation.** The release
        # profile's thin LTO is for the binary that ships: with it, every one of
        # the 114 test executables runs a thin-LTO link over the workspace
        # libraries, which was 17 of the 22 minutes of the workspace compile in
        # CI. The suite keeps what the release proof is for (optimiser on, debug
        # assertions and overflow checks off). The package and its dependencies
        # keep the profile as it is. Both suite derivations take this, because
        # cargo builds a dependency again when the profile differs.
        testProfile = {
          CARGO_PROFILE_RELEASE_LTO = "off";
        };

        # **The suite's dependencies are a second build, on purpose.** The
        # suite compiles dev-dependencies, which switch on features of the
        # shared dependencies. Cargo builds a crate once per feature set, so
        # one build serving both would leave the package recompiling
        # whichever half it did not match. `--all-targets` compiles the
        # test executables' dependencies for real.
        testArtifacts = craneLib.buildDepsOnly (
          common
          // testProfile
          // {
            inherit src;
            pname = "jojobot-tests";
            cargoExtraArgs = "--workspace --locked";
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
        # The same sandbox environment the package's check phase had.
        tests = craneLib.cargoTest (
          common
          // testProfile
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
              echo 'upgrade_gate omitted from the Nix sandbox: no git history or CI last-release cache; make check keeps the gate mandatory in CI.'
            '';
            # **The whole workspace runs here, the exercise crate included.** Its
            # free cases spawn the shipped server and a store of their own and
            # watch for the line that says the server is serving, which is the
            # harness every room stands on, so a sandbox that cannot run them
            # would be a sandbox that cannot run a room. They run here with the
            # rest. (The paid half of that crate is a binary somebody has to
            # invoke, and no test starts it.)
            #
            # fixture_roster's unpushed-commit check is the one kind of test
            # skipped, at test granularity: it reads `git log origin/main..HEAD`
            # against the checkout it runs in, and fails loud rather than passing
            # quietly when it cannot — by design, so a missing check is never
            # reported as a clean one. `src` here is a Nix store path: it has no
            # `.git`, the same reason JOJOBOT_BUILD above falls back to "unknown"
            # instead of reading `self.rev`. That makes the check unrunnable in
            # this sandbox on every build, not occasionally broken. The two
            # checks that read the commit range from the roster boundary to HEAD
            # are the same kind, for the same reason.
            #
            # All three stay enforced by `make check`, which is where they can
            # read the history.
            # The upgrade gate also needs the CI-owned last-release cache. Its
            # cases use the same test skips here; make check's ALWAYS_RUN gate
            # still runs in branch, pull-request and prepush jobs.
            #
            # **The compile prints a line for each unit it finishes, so the log
            # is not silent while it works.** `cargo test --release --workspace`
            # prints one `Compiling <package>` line when a package starts and
            # nothing more until the whole compile ends, so the workspace's own
            # large test binaries compile for half an hour with no output and
            # `--max-silent-time` kills a build that is alive. The JSON message
            # format prints one line for each artifact cargo finishes (the
            # human diagnostics stay human). Each line reports a real step, so a
            # build that hangs still prints nothing and the guard still kills it
            # (the Makefile's flake-check keeps 1800 seconds).
            cargoTestExtraArgs = builtins.concatStringsSep " " [
              "--message-format"
              "json-render-diagnostics"
              "--workspace"
              "--"
              "--skip=no_unpushed_commit_message_writes_a_pronoun_for_the_operator"
              "--skip=no_commit_message_since_the_boundary_quotes_an_off_roster_handle"
              "--skip=the_handle_check_range_reaches_head"
              "--skip=deaths::"
              "--skip=the_current_binary_boots_on_the_last_deployed_store"
              "--skip=the_last_deployed_store_recorded_six_hours_earlier_reads_back"
              "--skip=the_last_deployed_store_recorded_three_days_earlier_reads_back"
              "--skip=the_scan_for_oversized_records_finds_one_stored_over_the_ceiling"
              "--skip=the_wait_for_the_store_lock_lasts_as_long_as_the_lock_is_held_and_no_longer"
              "--skip=every_recorded_held_value_joins_to_an_entity"
              "--skip=a_held_value_the_oracle_cannot_join_fails_the_gate"
              "--skip=the_statement_splitter_keeps_a_semicolon_inside_a_value"
              "--skip=a_hand_made_instance_record_keeps_its_operator_and_zone_across_a_boot"
              "--skip=field_link_holds_the_links_the_writes_imply_and_nothing_else"
              "--skip=a_slow_statement_alert_is_not_a_degradation"
              "--skip=any_other_warning_or_error_is_still_a_degradation_beside_the_alert"
              "--skip=a_slow_acquire_alert_is_not_a_degradation"
              "--skip=any_other_warning_is_still_a_degradation_beside_the_acquire_alert"
              "--skip=a_pointer_at_a_record_the_build_supplies_boots_quiet_and_reads_back"
              "--skip=a_reference_field_naming_a_supplied_record_boots_quiet_and_reads_back"
              "--skip=the_boots_over_a_store_the_deployed_build_filled_log_no_warning_and_no_error"
              "--skip=a_boot_over_a_run_with_a_long_history_reaches_its_serving_line"
              "--skip=a_stored_cadence_days_rhythm_reads_the_same_on_the_head"
              "--skip=a_stored_cadence_days_rhythm_checks_in_on_the_head_as_it_did_before"
              "--skip=rows_classed_own_still_reach_their_things_on_the_last_deployed_store"
              "--skip=a_thread_a_deployed_build_left_over_the_default_ceiling_reads_and_edits_as_before"
              "--skip=a_view_stored_with_a_filter_record_answers_as_before_on_the_current_binary"
              "--skip=a_stored_mark_inside_code_reads_as_the_handle_on_the_current_binary"
              "--skip=a_lost_link_table_is_rebuilt_at_the_next_boot_on_the_last_deployed_store"
              "--skip=a_store_holding_a_row_at_a_handle_the_build_supplies_boots_and_the_row_answers"
              "--skip=what_the_operator_wrote_on_a_handle_the_build_stopped_supplying_reads_back_as_a_thing"
              "--skip=everything_naming_a_handle_the_build_stopped_supplying_follows_the_row_the_boot_gave_it"
              "--skip=deploy_8_1::a_dependency_list_rewritten_over_three_records_walks_as_the_newest_holds_it"
              "--skip=deploy_9_1::a_bot_is_not_told_again_a_note_it_held_before_the_upgrade"
              "--skip=deploy_9_1::the_current_binary_boots_with_the_link_rule_on_over_the_last_deployed_store"
              "--skip=deploy_9_2_1::read_mail_the_last_deployed_build_left_open_is_closed_by_the_batch_close"
              "--skip=runs_in_moment_order::the_runs_of_the_last_deployed_store_read_in_moment_order_on_the_head"
              "--skip=deploy_9_2_1::repair_pass::an_old_merge_under_a_later_one_gets_its_event_once_across_boots"
              "--skip=deploy_9_2_1::repair_pass::an_old_merge_chain_with_its_event_in_place_gets_none_written"
              "--skip=deploy_9_1::the_helper_that_shapes_a_recording_takes_one_marker_off_and_moves_no_row"
            ];
          }
        );
      in
      {
        packages.default = jojobot;
        checks = {
          inherit tests;

          # **The module evaluates with one app declared**, and the unit it
          # produces carries the apps file, the credential for the app's token
          # hash, and nothing of a token. The system is evaluated, not built.
          appsModule =
            let
              evaluated = nixpkgs.lib.nixosSystem {
                inherit system;
                modules = [
                  self.nixosModules.default
                  {
                    boot.isContainer = true;
                    system.stateVersion = "26.05";
                    services.jojobot = {
                      enable = true;
                      allowNoAuth = true;
                      package = jojobot;
                      doltPackage = dolt;
                      apps.springfield-ci = {
                        thread = "thread:deploys-alpha";
                        writableKeys = [
                          "commit"
                          "host"
                        ];
                        tokenHashFile = "/run/agenix/springfield-ci-token-hash";
                      };
                    };
                  }
                ];
              };
              unit = evaluated.config.systemd.services.jojobot;
              appsFile = unit.environment.JOJOBOT_APPS_FILE;
              credentials = unit.serviceConfig.LoadCredential;
            in
            pkgs.runCommand "jojobot-apps-module" { } ''
              set -eu
              test ${builtins.toJSON (credentials == [ "app-token-hash-springfield-ci:/run/agenix/springfield-ci-token-hash" ])} = true
              grep -q '"name":"springfield-ci"' ${appsFile}
              grep -q '"thread":"thread:deploys-alpha"' ${appsFile}
              grep -q '"keys":\["commit","host"\]' ${appsFile}
              grep -q '"revoked":false' ${appsFile}
              ! grep -qi 'token' ${appsFile}
              touch $out
            '';

          # **The module evaluates with the metrics listener on**, and the unit
          # names its address beside the main one, on a port of its own. With
          # it off, the unit names no metrics address at all.
          metricsModule =
            let
              evaluate =
                metrics:
                (nixpkgs.lib.nixosSystem {
                  inherit system;
                  modules = [
                    self.nixosModules.default
                    {
                      boot.isContainer = true;
                      system.stateVersion = "26.05";
                      services.jojobot = {
                        enable = true;
                        allowNoAuth = true;
                        package = jojobot;
                        doltPackage = dolt;
                        inherit metrics;
                      };
                    }
                  ];
                }).config.systemd.services.jojobot.environment;
              on = evaluate {
                enable = true;
                port = 9464;
              };
              off = evaluate { };
            in
            pkgs.runCommand "jojobot-metrics-module" { } ''
              set -eu
              test ${builtins.toJSON (on.JOJOBOT_METRICS_BIND == "127.0.0.1:9464")} = true
              test ${builtins.toJSON (on.JOJOBOT_BIND == "127.0.0.1:8080")} = true
              test ${builtins.toJSON (off ? JOJOBOT_METRICS_BIND)} = false
              touch $out
            '';
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
            # The bar needs both on the path: `scripts/sabotage` is a python
            # script, and the bar's history tests and size reports build and read
            # scratch repositories with git. The sandbox's check phase lists them
            # as `nativeCheckInputs` for the same reason, and a shell that lacked
            # them worked only on a machine that happened to have its own.
            pkgs.python3
            pkgs.git
            pkgs.curl
            # `scripts/ci-root` runs a pull-request job's command with the
            # checkout at one fixed path, through `unshare` and `mount`. The
            # host runner's own PATH has neither.
            pkgs.util-linux
          ];
          # Cargo's default ./target (gitignored) — no CARGO_TARGET_DIR override,
          # which would anchor to the shell-entry $PWD and leak artifacts if run
          # from a parent directory.

          # **sccache runs under a temp directory no run owns.** A host that
          # exports `RUSTC_WRAPPER` as sccache shares one server among every
          # build, and that server keeps the TMPDIR of whichever run started it
          # — a bar run's own, removed when the run ends, or a shell's, removed
          # when the shell does. Every build then fails with "Failed to create
          # temp dir" until the server is restarted. sccache has no temp setting
          # of its own, so the shell points the wrapper at a script that runs the
          # real sccache with a stable TMPDIR. A host with no sccache wrapper
          # is untouched.
          shellHook = ''
            case "''${RUSTC_WRAPPER:-}" in
              */sccache)
                export JOJOBOT_SCCACHE="$RUSTC_WRAPPER"
                export RUSTC_WRAPPER=${./scripts/sccache-stable-tmp}
                ;;
            esac
          '';
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
