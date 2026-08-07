{ config, lib, pkgs, ... }@args:

with lib;

let
  cfg = config.services.jojobot;
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
      '';
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
      // optionalAttrs cfg.allowNoAuth { JOJOBOT_ALLOW_NO_AUTH = "1"; };

      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jojobot";
        Restart = "on-failure";
        RestartSec = "5s";
        EnvironmentFile = mkIf (cfg.environmentFile != null) cfg.environmentFile;

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
