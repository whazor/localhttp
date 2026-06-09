{
  description = "localhttp development front door for localhost apps";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    , rust-overlay
    }:
    flake-utils.lib.eachDefaultSystem
      (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "clippy" "rustfmt" "rust-src" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "localhttp";
          version = "0.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                name = baseNameOf path;
              in
              !(name == "target" || name == ".git" || name == "result");
          };

          cargoHash = "sha256-brmY2ifcBGrCoEiGfJU4dLl4gp1tGtNrOZTkkNI50DU=";

          nativeBuildInputs = [ pkgs.makeWrapper ];

          postInstall = ''
            wrapProgram "$out/bin/localhttp" \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.mkcert ]}
          '';
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.cargo-watch
            pkgs.mkcert
            pkgs.just
          ];

          RUST_BACKTRACE = "1";
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
