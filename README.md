# localhttp

`localhttp` is a small Axum front door for local development apps.

It runs one HTTP/HTTPS reverse proxy on `:80` and `:443`, and the CLI registers
`*.localhost` names with the running daemon:

```sh
port="$(nix run path:. -- test-app)"
my-test-app --port "$port"
```

Opening `https://test-app.localhost/` proxies to the registered backend:

```text
http://127.0.0.1:<port>/
```

The backend receives `Host`, `X-Forwarded-Proto`, and `X-Forwarded-Host`
headers for the original `*.localhost` request.

Certificates are generated per app name and selected with SNI. Registering a new
app creates or refreshes that app's certificate when certificates have already
been initialized, and the running HTTPS server reloads certificates after they
change.

## Development

```sh
nix develop path:.
cargo test
```

More documentation lives in [docs/](docs/).

## Running

Run the front door as root so it can bind ports `80` and `443`:

```sh
sudo localhttp serve
```

Normal users can register routes with the same binary while `serve` is running:

```sh
port="$(localhttp test-app)"
my-test-app --port "$port"
```

## Certificates

The flake includes `mkcert`. Generate local trusted certificates:

```sh
nix run path:. -- certs
```

By default this hands certificates to the running `localhttp serve` process over
`/tmp/localhttp/serve.sock`. The server stores them under:

```text
/tmp/localhttp/certs/
```

For unprivileged development:

```sh
nix run path:. -- serve --http-only --http-addr 127.0.0.1:8080
```

Useful commands:

```sh
nix run path:. -- test-app
nix run path:. -- list
nix run path:. -- clear test-app
nix run path:. -- clear --all
```

## Notes

Routes are stored in the running daemon's memory and are cleared when
`localhttp serve` restarts. The control socket is intended for local development
machines; any local user who can access the socket can register routes or
install local certificates.
