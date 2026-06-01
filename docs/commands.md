# Command Reference

## `localhttp <app-name>`

Registers an app name with the running `localhttp serve` daemon and prints an
available backend port.

```sh
port="$(localhttp test-app)"
```

This registers:

```text
test-app.localhost -> http://localhost:<port>
```

App names are normalized to lowercase and may contain only ASCII letters,
digits, and `-`. The name cannot start or end with `-`.

Valid examples:

```text
api
test-app
checkout-2
```

Invalid examples:

```text
-api
api-
test_app
```

The generated port is allocated by `localhttp serve` by binding `127.0.0.1:0`,
checking the same port on `::1` when IPv6 loopback is available, then closing
the listeners. The backend app should bind the printed port immediately.

## `localhttp serve`

Runs the Axum reverse proxy server.

For the default privileged listeners, run it through a service manager. See
[Service Managers](service-managers.md) for systemd and launchd templates.

Default listeners:

```text
HTTP:  0.0.0.0:80
HTTPS: 0.0.0.0:443
```

Options:

```text
--http-addr <addr>      HTTP listen address
--https-addr <addr>     HTTPS listen address
--cert-file <path>      TLS certificate file
--key-file <path>       TLS private key file
--http-only             Run only the HTTP listener
```

Environment variables:

```text
LOCALHTTP_HTTP_ADDR
LOCALHTTP_HTTPS_ADDR
LOCALHTTP_CERT_FILE
LOCALHTTP_KEY_FILE
LOCALHTTP_CERT_DIR
LOCALHTTP_SERVE_SOCKET
LOCALHTTP_HTTP_ONLY
```

If HTTPS is enabled and no explicit certificate paths are supplied, `serve`
stores certificates in:

```text
/tmp/localhttp/certs/
```

User-side commands register routes and hand generated certificates to `serve`
over:

```text
/tmp/localhttp/serve.sock
```

Generate or refresh them with:

```sh
localhttp certs
```

## `localhttp certs`

Generates trusted local certificates through `mkcert`.

```sh
localhttp certs
```

The default mode connects to the running daemon and generates a localhost
fallback certificate plus one certificate per currently registered
`<app>.localhost` name. App certificates are selected by SNI when
`localhttp serve` runs without explicit `--cert-file` and `--key-file`.

Custom paths:

```sh
localhttp certs \
  --cert-file ./certs/localhttp.pem \
  --key-file ./certs/localhttp-key.pem
```

Both `--cert-file` and `--key-file` must be supplied together.

Environment variables:

```text
LOCALHTTP_CERT_FILE
LOCALHTTP_KEY_FILE
```

## `localhttp cert-info`

Prints certificate status from the running daemon.

```sh
localhttp cert-info
```

## `localhttp list`

Prints registered routes.

```sh
localhttp list
```

Example output:

```text
test-app.localhost -> http://localhost:42055
```

## `localhttp clear`

Removes routes from the running daemon's in-memory registry.

```sh
localhttp clear test-app
localhttp clear --all
```

Pass either a name or `--all`, not both.
