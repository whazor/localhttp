# localhttp documentation

`localhttp` gives local development apps stable `*.localhost` names while
letting each app run on its own random loopback port.

The current implementation is intentionally small:

- `localhttp <app-name>` registers `<app-name>.localhost` and prints a free port.
- `localhttp serve` runs the Axum reverse proxy server on HTTP and HTTPS.
- Requests for `<app-name>.localhost` proxy to `http://localhost:<port>`.
- `localhttp certs` creates trusted local certificates with `mkcert`.

## Pages

- [Quick start](quick-start.md)
- [Command reference](commands.md)
- [Configuration](configuration.md)
- [Service managers](service-managers.md)
- [TLS and privileged ports](tls-and-ports.md)
- [Architecture](architecture.md)
- [Development](development.md)

## Basic flow

Start the shared front door through systemd or launchd, then register app
routes as a normal user.

Register an app and start it on the returned port:

```sh
port="$(nix run path:. -- test-app)"
my-test-app --port "$port"
```

Open:

```text
https://test-app.localhost/
```

The backend receives a proxied request at:

```text
http://localhost:<port>/
```

For HTTPS requests, the proxied request includes:

```text
Host: test-app.localhost
X-Forwarded-Proto: https
X-Forwarded-Host: test-app.localhost
```
