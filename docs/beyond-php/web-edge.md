---
title: "Edge Deployment"
description: "Run an elephc --web origin behind Caddy, nginx, HAProxy, Traefik, Apache, Ferron, or a cloud load balancer."
sidebar:
  order: 8
---

`--web` is deliberately an **origin**, not a complete edge server. It speaks
HTTP/1.1 in cleartext and owns the PHP execution model: the compiled handler,
per-request state, sessions, and the worker/pool/request process isolation. TLS
termination, automatic certificates, HTTP/2-3, static files, virtual hosts, rate
limiting, and request observability belong to a separate edge process.

The origin is proxy-agnostic. Any server that can forward HTTP/1.1 to a Unix
domain socket or a loopback TCP address works; this page shows ready-to-adapt
configurations for the common ones.

## Why two processes

FrankenPHP embeds a PHP interpreter in a Go web server. elephc has no
interpreter to embed: the program is already native code. Embedding a web server
would add a large, release-coupled Rust dependency to every application binary
while leaving the PHP-specific work (sessions, superglobals, isolation)
untouched.

The two-process split keeps each side replaceable:

```text
client ──HTTPS / h2 / h3──▶ edge ──HTTP/1.1 / UDS or TCP──▶ elephc origin ──▶ PHP handler
```

The edge is not linked into the origin. The contract is plain HTTP/1.1 over a
Unix domain socket, or loopback TCP when a Unix socket is not available.

## Quick start

Compile the example and start it on a Unix socket:

```bash
cargo run -- --web examples/web-edge/main.php
./examples/web-edge/main --listen unix:/run/elephc/app.sock --workers 4
```

The example answers `GET /healthz` with `200 ok` and echoes the connection facts
(`REMOTE_ADDR`, `REMOTE_PORT`, `REQUEST_SCHEME`, `HTTPS`, `SERVER_PORT`,
`HTTP_HOST`) plus a session counter on every other path.

Then point your edge at `/run/elephc/app.sock`. For loopback TCP, start the
origin with `--listen 127.0.0.1:8080` instead.

## Choosing an edge

| Edge | Origin transport | Full example |
|---|---|---|
| Caddy | Unix socket or TCP | [`examples/web-edge/Caddyfile`](../../examples/web-edge/Caddyfile) |
| nginx | Unix socket or TCP | [`examples/web-edge/nginx.conf`](../../examples/web-edge/nginx.conf) |
| HAProxy | Unix socket or TCP | [`examples/web-edge/haproxy.cfg`](../../examples/web-edge/haproxy.cfg) |
| Traefik | TCP (file provider) | [`examples/web-edge/traefik.yml`](../../examples/web-edge/traefik.yml) and [`traefik-dynamic.yml`](../../examples/web-edge/traefik-dynamic.yml) |
| Apache httpd | Unix socket or TCP | [`examples/web-edge/apache.conf`](../../examples/web-edge/apache.conf) |
| Ferron 2.8.x (stable) | Unix socket or TCP | [`examples/web-edge/ferron/ferron-2.kdl`](../../examples/web-edge/ferron/ferron-2.kdl) |
| Ferron 3 RC (preview) | Unix socket or TCP | [`examples/web-edge/ferron/ferron-3.conf`](../../examples/web-edge/ferron/ferron-3.conf) |
| Cloud load balancer | TCP | Use the provider's HTTP/1.1 target group or backend service |

Caddy is the shortest configuration and the default in the container example.
nginx is the most common existing deployment. HAProxy is a good fit when the
edge already does L4/L7 balancing. Traefik is container-native. Apache is useful
when it is already the estate standard. Ferron is a Rust single-binary edge;
its 3.x line is still a release candidate and is labelled **PREVIEW**.

## Edge snippets

The full files are linked above; these are the parts that matter for the
origin.

### Caddy

```text
example.com {
    reverse_proxy unix//run/elephc/app.sock {
        health_uri /healthz
        health_interval 10s
    }
}
```

Caddy sets `X-Forwarded-For`, `X-Forwarded-Proto`, and `X-Forwarded-Host`
automatically. Use `reverse_proxy 127.0.0.1:8080` for loopback TCP; the
container TCP variant mounts [`Caddyfile.tcp`](../../examples/web-edge/Caddyfile.tcp).

### nginx

```text
location / {
    proxy_pass http://unix:/run/elephc/app.sock:/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Port $server_port;
    proxy_set_header X-Real-IP $remote_addr;
}
```

Use `proxy_pass http://127.0.0.1:8080;` for loopback TCP. Keep
`proxy_http_version 1.1` so the upstream connection is reusable.

### HAProxy

```text
backend elephc
    option httpchk GET /healthz
    server app /run/elephc/app.sock
    http-request set-header X-Forwarded-For %[src]
    http-request set-header X-Forwarded-Proto https
    http-request set-header X-Real-IP %[src]
```

HAProxy does not serve static files; put a static server or object store beside
it if the application needs one.

### Traefik

```yaml
http:
  routers:
    app:
      rule: Host(`example.com`)
      service: app
  services:
    app:
      loadBalancer:
        servers:
          - url: "http://127.0.0.1:8080"
```

The file-provider example uses loopback TCP because Unix-socket backend support
varies by Traefik version. Set `--trusted-proxy 127.0.0.1/32` on the origin when
using this layout.

### Apache httpd

```text
ProxyPass / unix:/run/elephc/app.sock|http://localhost/
ProxyPassReverse / unix:/run/elephc/app.sock|http://localhost/
RequestHeader set X-Forwarded-Proto "https"
RequestHeader set X-Forwarded-Port "443"
```

Requires `mod_proxy`, `mod_proxy_http`, and `mod_headers`. Use
`http://127.0.0.1:8080/` for loopback TCP.

### Ferron

Ferron 2.8.x is the stable line and uses KDL syntax; Ferron 3.x is a release
candidate with a new `.conf` syntax. The repository keeps the two files separate
on purpose:

- [`ferron-2.kdl`](../../examples/web-edge/ferron/ferron-2.kdl) — stable,
  KDL, verified against Ferron 2.8.1.
- [`ferron-3.conf`](../../examples/web-edge/ferron/ferron-3.conf) — **PREVIEW**,
  `.conf`, verified against Ferron 3.0.0-rc.10.

Both terminate TLS with ACME, serve `/static`, and proxy everything else to the
Unix socket. Ferron 3 sets the forwarded headers automatically; the Ferron 2
file sets them explicitly with `proxy_request_header_replace`. Do not mix the
two syntaxes in one file.

## Trust boundary

A proxy is not automatically trusted just because it can reach the origin. By
default, `REMOTE_ADDR` is the proxy address (or empty for a Unix socket),
`REQUEST_SCHEME` is `http`, `HTTPS` is absent, and `SERVER_PORT` is the bound
port (or `0`). This matches a PHP-FPM origin behind a proxy that does not pass
real-IP parameters; Symfony, Laravel, and other frameworks can still resolve the
client from `HTTP_X_FORWARDED_*` with their own trusted-proxy configuration.

To let the origin derive the facts itself, add the proxy to
`--trusted-proxy`:

```bash
./app --listen unix:/run/elephc/app.sock --trusted-proxy unix
./app --listen 127.0.0.1:8080 --trusted-proxy 127.0.0.1/32
```

The flag is repeatable and accepts IPv4/IPv6 addresses, CIDRs, or `unix`. Only
peers in the allowlist may supply `Forwarded` or `X-Forwarded-*`; other peers are
ignored entirely. RFC 7239 `Forwarded` wins over `X-Forwarded-*` when it is
well-formed. The chain is walked right-to-left through trusted hops, so the
rightmost untrusted client address becomes `REMOTE_ADDR`; `X-Forwarded-Proto`
drives `REQUEST_SCHEME`, `HTTPS=on` is exposed only for HTTPS, and
`X-Forwarded-Port`, a forwarded `host=` port, or the scheme default drives
`SERVER_PORT`. `HTTP_HOST` is not rewritten, and the raw forwarded headers stay
visible in `$_SERVER`.

Do not trust `0.0.0.0/0`. Use the narrowest CIDR that contains the proxy, or the
`unix` token when the proxy connects over a socket. See
[Trusted proxies](web.md#trusted-proxies) for the full PHP-FPM parity table.

## Health checks

`GET /healthz` is application code, not a server-level endpoint. Caddy, Ferron,
HAProxy, and other edges can use it for active or passive health checking. Keep
the route cheap: it should not touch sessions, databases, or upstream services
unless that is the health signal you actually want.

## Logging

The origin writes diagnostics and the optional `--access-log` lines to stderr;
the edge writes its own access and error logs. When the edge is in front, prefer
the edge's access log and leave `--access-log` off on the origin to avoid
duplicate lines. Container and systemd units should collect both streams
separately.

## Socket permissions

The origin creates the Unix socket with the process umask. Run the origin and
the edge under the same group (or a shared runtime directory) and choose a
matching umask. The systemd units use `RuntimeDirectory=elephc`,
`RuntimeDirectoryMode=0750`, and `UMask=0007`; the compose stacks mount a shared
named volume at `/run/elephc`.

The master unlinks the socket on a clean shutdown and replaces a stale socket
file at startup. A regular file or directory at the configured path is refused
instead of being deleted. Unix socket paths are limited to 100 bytes.

## systemd

The origin unit is
[`examples/web-edge/deploy/systemd/elephc-app.service`](../../examples/web-edge/deploy/systemd/elephc-app.service).
It runs the compiled binary with `--listen unix:/run/elephc/app.sock`, a shared
`elephc-web` group, a private runtime directory, and `SIGTERM` as the stop
signal.

Edge units and notes:

- Caddy: [`examples/web-edge/deploy/systemd/caddy.service`](../../examples/web-edge/deploy/systemd/caddy.service)
- Ferron: [`examples/web-edge/deploy/systemd/ferron.service`](../../examples/web-edge/deploy/systemd/ferron.service)
- nginx: use the distribution's `nginx.service` and install
  [`examples/web-edge/nginx.conf`](../../examples/web-edge/nginx.conf) as a site;
  `systemctl reload nginx` after certificate changes.

The edge does not supervise the origin. systemd (or Kubernetes) restarts the two
processes; the origin's own prefork master supervises its PHP workers. A restart
of either process is hidden from clients once the socket and the edge's upstream
retry/health behavior recover.

The origin installs `SIGINT`/`SIGTERM` handlers without `SA_RESTART`, forwards
`SIGTERM` to its workers, and reaps orphaned descendants through its `waitpid`
loop. A container therefore does not need `tini`; `init: true` is optional belt
and braces. The stop grace period must cover in-flight requests; the units use
`TimeoutStopSec=30`, and the application lifecycle drains active requests before
shutdown while the default request lifecycle terminates workers promptly.

## Containers

The container files live in
[`examples/web-edge/deploy/`](../../examples/web-edge/deploy/):

- `Dockerfile.app` builds the origin in a multi-stage image and runs the binary
  directly as PID 1 with `STOPSIGNAL SIGTERM`; there is no shell wrapper and no
  supervisord.
- `compose.yml` runs the origin plus **Caddy** with a shared named volume for
  `/run/elephc`; only Caddy publishes ports 80/443.
- `compose.nginx.yml` is the nginx variant.
- `compose.ferron.yml` is the Ferron 2.8.x variant.
- `compose.tcp.yml` is the loopback-TCP variant: the edge shares the app's
  network namespace, the app listens on `127.0.0.1:8080`, and
  `--trusted-proxy 127.0.0.1/32` makes the loopback proxy the only trusted peer.

The stacks use a one-shot init service to make the socket and session volumes
writable by the shared uid. Adapt the ids to your host policy. The Caddy and
Ferron variants run both services as uid 10001; the nginx image runs as root by
default, so it reaches the socket through the shared volume. Use an unprivileged
nginx image with high ports behind a load balancer if root is not acceptable.

## Out of scope

This profile deliberately does **not** embed a web server, use FastCGI, or add
in-process TLS/HTTP2 to the origin. The edge is a separate process; the origin
remains a standalone HTTP/1.1 server that can also run without any edge for
development or trusted internal networks.
