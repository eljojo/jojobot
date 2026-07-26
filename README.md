# jojobot

A personal-assistant server that fronts your existing life-layer services
(task/kanban, notes, calendar, link library) and exposes itself to an AI
assistant through a single [Model Context Protocol](https://modelcontextprotocol.io)
(MCP) endpoint.

jojobot is **not** an MCP proxy. It is software with a domain: it feeds the
assistant context, guards writes behind invariants, remembers what matters, and
never runs inference of its own — the assistant is the only mind.

> **Status: two domains live, and sessions can boot as an identity.** The
> server ships **Memory** — typed entities and dated facts with provenance
> (testimony vs inference), a write guard that blocks near-miss creations with
> candidates, full-text `search` with structured edges, aliases, and
> orienteering retrieval (every hit arrives with its surroundings) — and
> **Mailboxes** — kanban-backed message boxes with `new → read → processed`
> state (read ≠ processed; processed is a terminal archive; no delete verbs —
> the tool surface is pinned by test). On top of both, **bots**: an AI
> identity is an entity of kind `bot` with a charter, rules, memory and one
> owned mailbox, and `boot_bot` hands a session all of it in one call.
> All behind OAuth2 resource-server auth, every write verified by read-back.

## Vision & roadmap

jojobot's mission is to move an assistant's *method* out of prose (rules files,
skills, per-session caches) into software: versioned, tested, enforced. The
roadmap is a ladder of **capabilities** — each milestone is named "after this,
I can ___", never after infrastructure:

memory (M1) → search & edges (M2) → **mailboxes** (M3) → **bots** — AI
identities with a handle, charter, rules, memory and an owned mailbox (M4) →
sessions on the record (M5) → trace: claim → fact → receipt → evidence (M6) →
portraits (M7) → attention: rules, rhythms, nags (M8) → sessions boot *from*
jojobot (M9) → seeding: batteries included, overrides win (M10). Everything
up to and including **M4** is shipped.

Two layers hold it together: the **engine** (this repo — user-agnostic, golden
tests) and **bots** (data in the user's own store). The design docs live with
the user, not in-repo. AI sessions working here: read [CLAUDE.md](./CLAUDE.md)
first.

## Design

- **User-agnostic.** Nothing about any particular person is compiled in. Every
  instance-specific value — issuer, audience, hostnames, preferences — is
  runtime configuration or data the server reads, never code.
- **Domain-driven, ports-and-adapters.** A pure `jojobot-domain` crate owns the
  ubiquitous language; adapters and the MCP surface depend on it, never the
  reverse.
- **Fronts, never owns.** The underlying services stay the source of truth. An
  anti-corruption layer per service quarantines each one's quirks.
- **Resource server only.** jojobot validates bearer tokens; it never issues
  them. Bring your own OpenID Connect authorization server.

## Layout

```
crates/
  jojobot-domain     pure domain — bounded contexts as modules, no I/O, no MCP
  jojobot-adapters   anti-corruption layer per fronted service (Outline, Vikunja)
  jojobot-mcp        the MCP adapter — the only crate that imports rmcp
  jojobot            the binary: HTTP transport + resource-server auth
```

## Build & run

Development uses a Nix flake (a Rust toolchain, `pkg-config`, and OpenSSL):

```sh
nix develop            # drops you in a shell with the pinned toolchain
cargo build            # build the workspace
cargo test             # run the tests, including the auth golden tests
cargo run -p jojobot   # start the server
```

Or build the package directly:

```sh
nix build              # produces ./result/bin/jojobot
```

## Deploying

The flake exposes an overlay and a NixOS module:

```nix
{
  nixpkgs.overlays = [ inputs.jojobot.overlays.default ];
  imports = [ inputs.jojobot.nixosModules.default ];

  services.jojobot = {
    enable = true;
    issuer = "https://id.example.org";       # enables resource-server auth
    resource = "https://jojobot.example.org/mcp";
    # audience defaults to the resource id — set it if the issuer's `aud` differs
  };
}
```

The service binds localhost by default; front it with a TLS-terminating reverse
proxy or tunnel. With `issuer` unset it runs open (development only).

## Configuration

All configuration is environment-driven.

| Variable | Meaning | Default |
| --- | --- | --- |
| `JOJOBOT_BIND` | Listen address | `127.0.0.1:8080` |
| `JOJOBOT_RESOURCE` | This server's resource identifier (RFC 9728) | derived from bind |
| `JOJOBOT_ISSUER` | OIDC issuer URL — **set this to enable auth** | unset |
| `JOJOBOT_AUDIENCE` | Required token audience (RFC 8707) | the resource id |
| `JOJOBOT_JWKS_URI` | Explicit JWKS URI | discovered from issuer |
| `JOJOBOT_ALLOW_NO_AUTH` | Set to `1` to run **without auth** (dev only) | unset |
| `JOJOBOT_ALLOWED_SUBJECTS` | Optional comma-separated `sub` allowlist (requires auth) | unset = any valid token |
| `JOJOBOT_OUTLINE_URL` / `JOJOBOT_OUTLINE_TOKEN` | The Outline instance Memory fronts | unset |
| `JOJOBOT_VIKUNJA_URL` / `JOJOBOT_VIKUNJA_TOKEN` | The Vikunja instance Mailboxes front | unset |

The server **fails closed**: with `JOJOBOT_ISSUER` unset it refuses to start
unless `JOJOBOT_ALLOW_NO_AUTH=1` is set explicitly, and even then it refuses a
non-loopback bind. This turns a dropped-secret misconfiguration into a startup
error rather than a silently unauthenticated `/mcp`.

## Auth model

jojobot is an OAuth2 protected resource per the MCP authorization spec:

- `GET /.well-known/oauth-protected-resource` advertises the authorization
  server(s), unauthenticated (RFC 9728).
- `/mcp` requires a valid bearer JWT. Tokens are verified against the issuer's
  JWKS with an **RS256-only** allowlist, and checked for issuer, audience
  (RFC 8707), and expiry. A 401 carries a `WWW-Authenticate` challenge pointing
  back at the metadata endpoint.

## License

[AGPL-3.0-or-later](./LICENSE).
