# TLS and Privileged Ports

## Local TLS

`localhttp certs` shells out to `mkcert`.

It first runs:

```sh
mkcert -install
```

Then it creates a localhost fallback certificate for:

```text
localhost
127.0.0.1
::1
```

It also creates one certificate per registered app, such as:

```text
finance.localhost
```

Apps use exact subject names because some clients do not accept wildcard
localhost certificates consistently.

Default server-owned certificate directory:

```text
/tmp/localhttp/certs/
```

User-side commands generate certificates in a private temporary directory and
hand them to `localhttp serve` over `/tmp/localhttp/serve.sock`. The server
validates and installs them, then reloads the SNI certificate set after files
change.

## Ports `80` and `443`

The default server addresses are:

```text
0.0.0.0:80
0.0.0.0:443
```

On Linux, binding ports below `1024` generally requires one of:

- running as root
- a service manager that runs the daemon with privileges
- a capability such as `CAP_NET_BIND_SERVICE`
- a local firewall or proxy rule that forwards privileged ports to unprivileged ports

The current binary binds its sockets directly. It does not yet include systemd
socket activation. See [Service Managers](service-managers.md) for systemd and
launchd service templates.

## Unprivileged development mode

Use HTTP-only mode on a high port:

```sh
localhttp serve --http-only --http-addr 127.0.0.1:8080
```

Then request with an explicit port:

```text
http://test-app.localhost:8080/
```

## Browser Behavior

The server keeps the browser on the `*.localhost` URL and proxies the request to
the registered backend.

For a request like:

```text
https://test-app.localhost/some/path?x=1
```

The backend request target is:

```text
http://127.0.0.1:<registered-port>/some/path?x=1
```

The proxy preserves the path and query string and sends forwarding headers such
as:

```text
Host: test-app.localhost
X-Forwarded-Proto: https
X-Forwarded-Host: test-app.localhost
```
