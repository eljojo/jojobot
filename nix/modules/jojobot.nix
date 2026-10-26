{ config, lib, pkgs, ... }@args:

with lib;

let
  cfg = config.services.jojobot;

  # **The apps file the server reads at boot.** One entry for each registered
  # app: its name, the one operation it may do, the thread it records under, the
  # kind it writes and the keys it may send. The token's hash is not in it; the
  # hash arrives as a credential (see `LoadCredential` below), so nothing a
  # token could be derived from is in the store.
  appsFile = pkgs.writeText "jojobot-apps.json" (
    builtins.toJSON (
      mapAttrsToList (name: app: {
        inherit name;
        operation = "record_deploy_event";
        thread = app.thread;
        kind = "event";
        keys = app.writableKeys;
        revoked = app.revoked;
      }) cfg.apps
    )
  );
in
{
  options.services.jojobot = {
    enable = mkEnableOption "jojobot personal-assistant MCP server";

    listenAddress = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = ''
        Address to bind to. Keep this on localhost when fronted by a reverse
        proxy / tunnel that terminates TLS (the intended deployment).
      '';
    };

    port = mkOption {
      type = types.port;
      default = 8080;
      description = "Port to listen on.";
    };

    metrics = {
      enable = mkEnableOption ''
        a second listener that serves GET /metrics for a Prometheus-compatible
        scraper. It serves jojobot's own numbers (calls, refusals, latency,
        sessions, mail waiting, things per kind) and none of the data it holds
      '';

      listenAddress = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = ''
          Address the metrics listener binds to. It is a listener of its own so
          that it is not behind the public proxy the main listener sits behind:
          keep it on an address only the scraper reaches.
        '';
      };

      port = mkOption {
        type = types.port;
        example = 9464;
        description = ''
          Port the metrics listener binds to. It must differ from `port`: jojobot
          refuses to start when the two are equal.
        '';
      };
    };

    issuer = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "https://id.example.org";
      description = ''
        OIDC issuer URL. When set, jojobot enforces resource-server auth on
        /mcp, validating bearer JWTs against the issuer's JWKS. When null,
        auth is DISABLED (development only) and /mcp is open.
      '';
    };

    audience = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Required token audience (RFC 8707). Must match exactly what the issuer
        places in the token's `aud` claim. Defaults to the resource id.
      '';
    };

    resource = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "https://jojobot.example.org/mcp";
      description = ''
        This server's public resource identifier, advertised in the RFC 9728
        protected-resource metadata. Set this to the public URL when behind a
        proxy; otherwise it is derived from the bind address.
      '';
    };

    jwksUri = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Explicit JWKS URI. Discovered from the issuer when null.";
    };

    allowNoAuth = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Explicitly permit running without authentication (development only).
        Required when `issuer` is null — otherwise the service fails closed and
        refuses to start. Even when true, a non-loopback bind is refused.
      '';
    };

    logLevel = mkOption {
      type = types.str;
      default = "info";
      description = "RUST_LOG value for the service.";
    };

    environmentFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Optional path to an environment file (e.g. an agenix secret) with
        additional JOJOBOT_* variables. Values here override the options above.

        The browser UI is switched on here and nowhere else: set both
        JOJOBOT_UI_CLIENT_ID (the OAuth client id registered at the issuer) and
        JOJOBOT_UI_BASE_URL (the origin, scheme included, a browser reaches this
        server on), with `issuer` set. JOJOBOT_ALLOWED_SUBJECTS, a comma-separated
        list of token subjects, limits who may log in; the operator's mailbox page
        is served only when it names one person. A JOJOBOT_UI_CLIENT_SECRET is
        refused at start. JOJOBOT_UI_CLIENT_ID must differ from the audience /mcp
        requires (`audience`, which defaults to the resource id); the server
        refuses to start when they are equal.

        Pushes to the operator's phone are switched on here too, over ntfy: set
        both JOJOBOT_NTFY_URL (the ntfy server, scheme included) and
        JOJOBOT_NTFY_TOPIC, and every message posted to the operator's mailbox is
        pushed to that topic. JOJOBOT_NTFY_TOKEN is the access token when the topic
        needs one; it is a credential, so it belongs in this file and nowhere else.
        Setting only one of the URL and the topic is refused at start, and setting
        neither pushes nothing.

        The link rule is on unless this file says otherwise: set
        JOJOBOT_LINK_RULE=log-only to roll it back, so a field value's links are
        computed and logged and nothing is marked. Any other word refuses to start.
      '';
    };

    apps = mkOption {
      default = { };
      description = ''
        The outside programs allowed to write to jojobot, each with a token of
        its own and one grant it cannot widen. An app is not a bot: it has no
        session, charter, mailbox or lease, and nothing it sends can start, stop
        or message a session. Today the one thing an app can do is record a
        deploy event under a thread, with `POST /app/v1/deploy-events`.

        Registering, changing and revoking an app is a change to this option
        that you deploy; there is no other way in. The route exists only when at
        least one app is declared.

        Make a token with `jojobot app-token`. It prints a fresh token and its
        hash and touches no store. Give the token to the app. Put only the hash
        in the secret that `tokenHashFile` names. The token itself is never in
        nix, the store or a log.
      '';
      example = literalExpression ''
        {
          springfield-ci = {
            thread = "thread:deploys-alpha";
            writableKeys = [ "commit" "host" "unit" ];
            tokenHashFile = "/run/agenix/springfield-ci-token-hash";
          };
        }
      '';
      type = types.attrsOf (types.submodule {
        options = {
          thread = mkOption {
            type = types.str;
            example = "thread:deploys-alpha";
            description = ''
              The handle of the thread every event of this app is recorded under.
              It must already exist in jojobot. The app cannot name another.
            '';
          };
          writableKeys = mkOption {
            type = types.listOf types.str;
            example = [ "commit" "host" "unit" ];
            description = ''
              The keys the app may send. A key jojobot guards, reserves or reads to
              decide when something is owed cannot be granted, and the server
              refuses to start if one is listed.
            '';
          };
          revoked = mkOption {
            type = types.bool;
            default = false;
            description = ''
              Switch the app off. A revoked app writes nothing, and its token is
              refused, after the next restart.
            '';
          };
          tokenHashFile = mkOption {
            type = types.str;
            example = "/run/agenix/springfield-ci-token-hash";
            description = ''
              Path to a file that holds only the hash of the app's token, for
              example an agenix secret. Write the secret's runtime path as a
              string: a path written as a literal in a nix file is copied into
              the store. The service manager loads it with `LoadCredential`, so
              the service reads it and nothing else does.
            '';
          };
        };
      });
    };

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Open the TCP port in the firewall (not needed behind a tunnel).";
    };

    package = mkOption {
      type = types.package;
      default = pkgs.jojobot or (throw ''
        jojobot package not found in pkgs.
        Add the overlay to your nixpkgs.overlays:
          nixpkgs.overlays = [ inputs.jojobot.overlays.default ];
      '');
      description = "The jojobot package to run.";
    };

    doltPackage = mkOption {
      type = types.package;
      # `args.dolt`, not `pkgs.dolt`: reading it out of the host's `pkgs`
      # would take whatever version that host's channel happens to carry,
      # which is the module choosing after all. The host hands it in.
      default = args.dolt or (throw ''
        dolt was not passed to this module.
        jojobot runs the store itself and pins no version of it: the deploying
        host chooses one and hands it in as a module argument.
          nixpkgs.lib.nixosSystem {
            specialArgs = { dolt = <the dolt package this host tracks>; };
            modules = [ inputs.jojobot.nixosModules.default ./configuration.nix ];
          }
        Or set services.jojobot.doltPackage directly.
      '');
      description = ''
        The dolt package jojobot runs the mailbox and session store with.

        It arrives as a module argument from the deploying host. This module
        names no version and no channel: pinning one here would decide, for
        every host, when the store's engine moves.
      '';
    };
  };

  config = mkIf cfg.enable {
    assertions = mapAttrsToList (name: _: {
      assertion = builtins.match "[a-z][a-z0-9-]{0,31}" name != null;
      message = "services.jojobot.apps.${name}: an app name is 1 to 32 lowercase letters, digits or hyphens, starting with a letter.";
    }) cfg.apps;

    systemd.services.jojobot = {
      description = "jojobot personal-assistant MCP server";
      documentation = [ "https://github.com/eljojo/jojobot" ];
      # Needs the network to fetch the issuer JWKS at startup when auth is on.
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      # **The store's binary is jojobot's to run, so it is on jojobot's PATH.**
      # It is not a service somebody administers beside jojobot: jojobot spawns
      # it, waits for it, and stops it with itself. The package is the host's
      # choice — see `doltPackage`.
      path = [ cfg.doltPackage ];

      environment = {
        JOJOBOT_BIND = "${cfg.listenAddress}:${toString cfg.port}";
        RUST_LOG = cfg.logLevel;
      }
      // optionalAttrs (cfg.issuer != null) { JOJOBOT_ISSUER = cfg.issuer; }
      // optionalAttrs (cfg.audience != null) { JOJOBOT_AUDIENCE = cfg.audience; }
      // optionalAttrs (cfg.resource != null) { JOJOBOT_RESOURCE = cfg.resource; }
      // optionalAttrs (cfg.jwksUri != null) { JOJOBOT_JWKS_URI = cfg.jwksUri; }
      // optionalAttrs cfg.allowNoAuth { JOJOBOT_ALLOW_NO_AUTH = "1"; }
      // optionalAttrs (cfg.apps != { }) { JOJOBOT_APPS_FILE = "${appsFile}"; }
      // optionalAttrs cfg.metrics.enable {
        JOJOBOT_METRICS_BIND = "${cfg.metrics.listenAddress}:${toString cfg.metrics.port}";
      };

      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jojobot";
        Restart = "on-failure";
        RestartSec = "5s";

        # **A stop signals the server alone.** The default, `control-group`,
        # signals every process of the unit at once, so the store dies at the
        # signal and the drain window has no store to finish its calls with.
        # `mixed` signals the main process, which drains and then stops the
        # store itself, and kills what is left once the main process has gone.
        KillMode = "mixed";
        EnvironmentFile = mkIf (cfg.environmentFile != null) cfg.environmentFile;

        # **Each app's token hash is a credential.** The service reads it from
        # its credentials directory and nothing else on the host can.
        LoadCredential = mapAttrsToList (
          name: app: "app-token-hash-${name}:${app.tokenHashFile}"
        ) cfg.apps;

        # **The store's data directory.** jojobot keeps mailboxes and sessions
        # in a SQL store it runs itself, so it is no longer stateless: this is
        # `/var/lib/jojobot`, and the store lives in `db` under it. With
        # `DynamicUser` systemd owns the real path and hands jojobot a stable
        # one, so nothing here has to know where it actually sits.
        StateDirectory = "jojobot";

        # Hardening. jojobot needs outbound HTTPS (JWKS), an inbound socket,
        # its state directory, and a child process serving the store on
        # loopback.
        DynamicUser = true;
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictRealtime = true;
        RestrictNamespaces = true;
        LockPersonality = true;
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];
        SystemCallFilter = [ "@system-service" ];
        SystemCallErrorNumber = "EPERM";

        # The store's server is a child process with a working set of its
        # own, and it is a Go program rather than a thread. Both ceilings are
        # jojobot's plus the server's, not jojobot's alone.
        MemoryMax = "1G";
        TasksMax = 256;
      };
    };

    networking.firewall.allowedTCPPorts =
      mkIf (cfg.openFirewall && cfg.listenAddress != "127.0.0.1") [ cfg.port ];
  };
}
