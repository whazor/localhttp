# localhttp

`localhttp` is a CLI for giving local development apps stable
`*.localhost` URLs.

It has two parts:

- `localhttp serve` runs one HTTP/HTTPS reverse proxy on `:80` and `:443`.
- `localhttp <app-name>` registers an app name with the running daemon and
  prints a backend port for that app.

Example:

```sh
port="$(localhttp test-app)"
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

More documentation lives in [docs/](docs/).

## Start The Daemon

Run `localhttp serve` with your service manager so it can bind ports `80` and
`443` without keeping a manual root shell around:

```sh
# Linux, after installing contrib/systemd/localhttp.service
sudo systemctl enable --now localhttp.service

# macOS, after installing contrib/launchd/dev.localhttp.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/dev.localhttp.plist
```

Normal users can register routes with the same binary while `serve` is running:

```sh
port="$(localhttp test-app)"
my-test-app --port "$port"
```

See [Service Managers](docs/service-managers.md) for systemd and launchd
templates.

## Certificates

Generate local trusted certificates:

```sh
localhttp certs
```

By default this hands certificates to the running `localhttp serve` process over
`/tmp/localhttp/serve.sock`. The server stores them under:

```text
/tmp/localhttp/certs/
```

For unprivileged development:

```sh
localhttp serve --http-only --http-addr 127.0.0.1:8080
```

Useful commands:

```sh
localhttp test-app
localhttp list
localhttp clear test-app
localhttp clear --all
```

## Notes

Routes are stored in the running daemon's memory and are cleared when
`localhttp serve` restarts. The control socket is intended for local development
machines; any local user who can access the socket can register routes or
install local certificates.
