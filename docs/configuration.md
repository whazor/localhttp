# Configuration

`localhttp` is configured through command-line flags and environment variables.
Flags take precedence because they are parsed directly by the CLI.

## Control Socket

User-side commands communicate with the running `localhttp serve` process over a
Unix socket.

Control socket resolution order:

1. `$LOCALHTTP_SERVE_SOCKET`
2. `/tmp/localhttp/serve.sock`

The daemon owns the route registry in memory. There is no shared route file, and
routes are cleared when `localhttp serve` restarts.

Example:

```sh
sudo localhttp serve
```

Then, in another shell:

```sh
port="$(localhttp test-app)"
```

## Certificate Directory

User-side commands run `mkcert` and send generated leaf certificates to
`localhttp serve` over the control socket. The server writes the final
certificate files.

Certificate directory resolution order:

1. `$LOCALHTTP_CERT_DIR`
2. `/tmp/localhttp/certs/`

Default directory:

```text
/tmp/localhttp/certs/
```

`localhttp certs` writes a localhost fallback certificate plus one certificate
per currently registered `<app>.localhost` name. Later `localhttp <app-name>`
registrations create or refresh that app's certificate. `localhttp serve`
selects the correct app certificate with SNI and reloads the SNI certificate set
while it is running.

## Listener Addresses

`serve` reads these environment variables:

```text
LOCALHTTP_HTTP_ADDR
LOCALHTTP_HTTPS_ADDR
LOCALHTTP_HTTP_ONLY
```

Examples:

```sh
LOCALHTTP_HTTP_ADDR=127.0.0.1:8080 localhttp serve --http-only
```

```sh
LOCALHTTP_HTTP_ADDR=0.0.0.0:80 \
LOCALHTTP_HTTPS_ADDR=0.0.0.0:443 \
localhttp serve
```

## TLS Files

`serve` and `certs` read:

```text
LOCALHTTP_CERT_FILE
LOCALHTTP_KEY_FILE
```

Both paths must be set together.

Example:

```sh
export LOCALHTTP_CERT_FILE="$HOME/.config/localhttp/cert.pem"
export LOCALHTTP_KEY_FILE="$HOME/.config/localhttp/key.pem"

localhttp certs
localhttp serve
```
