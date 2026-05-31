# localhttp Architecture

`localhttp` has two execution contexts:

- user shell commands generate certificates and send control requests
- root `serve` owns runtime state, binds privileged ports, and proxies browser traffic

```mermaid
flowchart LR
    User["User shell"] --> CLI["localhttp <app-name>"]
    CLI --> Control["/tmp/localhttp/serve.sock"]
    CLI --> Mkcert["mkcert"]
    Mkcert --> TempCert["temporary leaf cert/key"]
    TempCert --> Control

    subgraph Serve["root: localhttp serve"]
        Socket["control socket listener"]
        Routes["in-memory route map"]
        Proxy["HTTP/HTTPS proxy<br/>:80/:443"]
        Installer["cert installer"]
        Resolver["Rustls SNI resolver"]
    end

    Control --> Socket
    Socket --> Routes
    Socket --> Installer
    Installer --> Certs["/tmp/localhttp/certs/*.pem"]
    Proxy --> Routes
    Proxy --> Certs
    Proxy --> Backend["127.0.0.1:<registered-port>"]

    Browser["Browser / curl"] --> DNS["<app>.localhost resolves to loopback"]
    DNS --> Proxy
```

## Runtime Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI as localhttp <app-name>
    participant Serve as root localhttp serve
    participant Mkcert as mkcert
    participant Browser
    participant App as backend app

    User->>CLI: localhttp test-app
    CLI->>Serve: Register(test-app) over /tmp/localhttp/serve.sock
    Serve->>Serve: allocate port and store route in memory
    Serve-->>CLI: port
    CLI->>Mkcert: generate test-app.localhost leaf cert
    Mkcert-->>CLI: temporary test-app.localhost.pem/key
    CLI->>Serve: InstallCert over /tmp/localhttp/serve.sock
    Serve->>Serve: validate and install cert/key
    CLI-->>User: print port only

    User->>App: start app on printed port
    Browser->>Serve: GET https://test-app.localhost/
    Serve->>Serve: select cert by SNI test-app.localhost
    Serve->>Serve: read test-app route from memory
    Serve->>App: proxy HTTP request
    Note over Serve,App: Host: test-app.localhost<br/>X-Forwarded-Proto: https<br/>X-Forwarded-Host: test-app.localhost
    App-->>Serve: response
    Serve-->>Browser: response
```

## State Model

Routes are process-local state owned by `localhttp serve`.

```text
CLI commands       -> /tmp/localhttp/serve.sock -> in-memory route map
Browser requests   -> :80/:443                  -> in-memory route map
```

There is no shared route file. Restarting `localhttp serve` clears registered
routes, so apps should re-register after the daemon restarts.

Certificates remain files because Rustls loads them from disk and `mkcert`
produces PEM files. The server owns final writes into:

```text
/tmp/localhttp/certs/
```

## Serve Process

```mermaid
flowchart TD
    Start["localhttp serve"] --> Control["bind control socket<br/>/tmp/localhttp/serve.sock"]
    Start --> HTTP["bind HTTP<br/>0.0.0.0:80"]
    Start --> HTTPS["bind HTTPS<br/>0.0.0.0:443"]

    Control --> Register["register/list/clear routes"]
    Control --> Install["validate and install certs"]
    Register --> Routes["in-memory route map"]
    Install --> CertDir["cert directory"]

    HTTPS --> Load["load cert directory"]
    Routes --> SNI["build Rustls SNI resolver"]
    CertDir --> SNI
    SNI --> Watch["periodically reload cert set"]

    HTTP --> Proxy["Axum fallback proxy"]
    HTTPS --> Proxy
    Proxy --> Host["read Host header"]
    Host --> Match["match <name>.localhost"]
    Match --> Routes
    Routes --> Route{"route exists?"}
    Route -- no --> NotFound["404"]
    Route -- yes --> Forward["proxy to http://127.0.0.1:<port>"]
```

## Serve Module Layers

```text
src/serve.rs
  process orchestration: bind HTTP/HTTPS, create shared state, start control socket

src/serve/control.rs
  control plane: register/list/clear routes, install certs, report cert info

src/serve/proxy.rs
  data plane: host matching, in-memory route lookup, URI/header rewrite

src/serve/tls.rs
  TLS runtime: static cert loading, SNI resolver construction, periodic reloads

src/serve/certs.rs
  certificate primitives: cert directory permissions, PEM parsing, host validation
```

## Paths And Environment

```mermaid
flowchart LR
    SocketEnv["LOCALHTTP_SERVE_SOCKET"] --> SocketPath["control socket"]
    TmpSocket["/tmp/localhttp/serve.sock"] --> SocketPath

    CertEnv["LOCALHTTP_CERT_DIR"] --> CertPath["cert directory"]
    TmpCerts["/tmp/localhttp/certs"] --> CertPath

    ServeEnv["sudo env<br/>LOCALHTTP_SERVE_SOCKET=/tmp/localhttp/serve.sock<br/>LOCALHTTP_CERT_DIR=/tmp/localhttp/certs"] --> RootServe["root localhttp serve"]
```
