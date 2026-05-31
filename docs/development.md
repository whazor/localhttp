# Development

## Nix

The flake pins:

```text
github:NixOS/nixpkgs/nixos-25.11
github:oxalica/rust-overlay
```

Enter the shell:

```sh
nix develop path:.
```

Build the package:

```sh
nix build path:.
```

Run the package:

```sh
nix run path:. -- --help
```

The `path:.` form works even when the directory has not been initialized as a
normal Git repository. In a regular Git repository, `nix develop`, `nix build`,
and `nix run` also work.

## Cargo

Format:

```sh
cargo fmt
```

Check:

```sh
cargo check
```

Test:

```sh
cargo test
```

## Dependency hash

The Nix package uses `buildRustPackage` with a fixed `cargoHash`.

When `Cargo.lock` changes, update the hash by temporarily setting:

```nix
cargoHash = pkgs.lib.fakeHash;
```

Then run:

```sh
nix build path:.
```

Nix will print the expected hash. Put that value back into `flake.nix`.

## Useful manual checks

Run `localhttp` on an unprivileged port:

```sh
cargo run -- serve --http-only --http-addr 127.0.0.1:8080
```

Leave that process running and use another shell for the remaining commands.

Register a route:

```sh
port="$(cargo run --quiet -- test-app)"
```

List routes:

```sh
cargo run --quiet -- list
```

Run a local server on the registered port:

```sh
python3 -m http.server "$port" --bind 127.0.0.1
```

Check the proxy:

```sh
curl -i http://test-app.localhost:8080/
```
