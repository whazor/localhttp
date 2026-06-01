# Architecture

`localhttp` is one binary with two roles:

- `localhttp serve` is the long-running daemon and owns runtime state
- CLI commands connect to the daemon over a Unix control socket

## Components

```text
localhttp serve
  -> bind /tmp/localhttp/serve.sock
  -> keep an in-memory map of <name> -> port
  -> bind HTTP/HTTPS listeners
  -> proxy browser requests to registered backend ports

localhttp <app-name>
  -> ask serve to allocate and register a port
  -> generate a mkcert leaf certificate
  -> send the certificate to serve
  -> print the port
```

## Route Registry

Routes are held in memory by `localhttp serve`. CLI commands do not write a
shared route file; they send commands to the daemon.

Restarting `localhttp serve` clears all routes. Apps should re-register after a
daemon restart.

## Host Matching

The server only handles hostnames ending in `.localhost`.

Examples:

```text
test-app.localhost
test-app.localhost:443
```

Both map to route name:

```text
test-app
```

Requests for `localhost` itself, unknown route names, or non-`.localhost`
hosts return `404 Not Found`.

## Reverse Proxying

The server proxies HTTP requests to the backend. The browser address stays on:

```text
https://test-app.localhost/
```

The backend receives requests at:

```text
http://localhost:<port>/
```

For a TLS request to `https://test-app.localhost/`, the proxied request includes:

```text
Host: test-app.localhost
X-Forwarded-Proto: https
X-Forwarded-Host: test-app.localhost
```

WebSocket proxying is not implemented.

## Port Allocation Caveat

The daemon asks the OS for a free port by binding `127.0.0.1:0`, checking the
same port on `::1` when IPv6 loopback is available, and closing the listeners.

Because the listener is closed before the backend starts, there is a small race:
another process could claim the port. In normal development usage this is rare,
but app launchers should bind the printed port immediately.
