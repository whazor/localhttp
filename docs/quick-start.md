# Quick Start

## Build or run from the checkout

```sh
nix build path:.
nix run path:. -- --help
```

The docs use `nix run path:. -- ...` for commands run from this checkout. If
`localhttp` is already installed in `PATH`, use `localhttp ...` instead.

## Run the front door

Binding directly to ports `80` and `443` usually requires privileges:

```sh
sudo nix run path:. -- serve
```

For unprivileged development, use an alternate HTTP port:

```sh
nix run path:. -- serve --http-only --http-addr 127.0.0.1:8080
```

## Generate local TLS certificates

```sh
nix run path:. -- certs
```

This installs the local `mkcert` CA if needed and hands certificates to the
running `localhttp serve` process over `/tmp/localhttp/serve.sock`. The server
stores them under:

```text
/tmp/localhttp/certs/
```

The certificate covers:

- `localhost`
- `127.0.0.1`
- `::1`

Each registered app also gets its own exact-name certificate, such as
`test-app.localhost`.

## Register and start an app

```sh
port="$(nix run path:. -- test-app)"
my-test-app --port "$port"
```

The registration command asks the running daemon for a currently free port and
stores this route in daemon memory:

```text
test-app.localhost -> http://127.0.0.1:<port>
```

The port is not reserved after registration returns. Start the app promptly so
another process does not claim the same port first.

## Check registered apps

```sh
nix run path:. -- list
```

## Remove routes

```sh
nix run path:. -- clear test-app
nix run path:. -- clear --all
```
