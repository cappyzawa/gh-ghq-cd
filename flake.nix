{
  description = "GitHub CLI extension to fuzzy find and cd to a ghq managed repository";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      # Shared package definition
      mkPackage = { pkgs, rustPlatform, target ? null }:
        let
          basePackage = {
            pname = "gh-ghq-cd";
            version = "0.9.1";
            src = ./.;
            cargoHash = "sha256-CLJ+Yz5lSedSHGNITgq19D13IXILm7N+DuF9CvDXcvs=";
            meta = {
              description = "GitHub CLI extension to fuzzy find and cd to a ghq managed repository";
              homepage = "https://github.com/cappyzawa/gh-ghq-cd";
              license = pkgs.lib.licenses.mit;
              mainProgram = "gh-ghq-cd";
            };
          };
        in
        rustPlatform.buildRustPackage (basePackage // pkgs.lib.optionalAttrs (target != null) {
          CARGO_BUILD_TARGET = target;
        });

      # Helper to create rust platform with overlay
      mkRustPlatform = pkgs:
        let
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [
              "x86_64-unknown-linux-musl"
              "aarch64-unknown-linux-musl"
            ];
          };
        in
        pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustPlatform = mkRustPlatform pkgs;
      in
      {
        packages = {
          # Native build for each system
          gh-ghq-cd = mkPackage { inherit pkgs rustPlatform; };
          default = self.packages.${system}.gh-ghq-cd;
        } // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # Linux musl targets (static binaries) - built on x86_64-linux
          x86_64-linux-musl =
            let
              pkgsCross = import nixpkgs {
                inherit system overlays;
                crossSystem = {
                  config = "x86_64-unknown-linux-musl";
                };
              };
              crossRustPlatform = mkRustPlatform pkgsCross;
            in
            mkPackage {
              pkgs = pkgsCross;
              rustPlatform = crossRustPlatform;
              target = "x86_64-unknown-linux-musl";
            };

          aarch64-linux-musl =
            let
              pkgsCross = import nixpkgs {
                inherit system overlays;
                crossSystem = {
                  config = "aarch64-unknown-linux-musl";
                };
              };
              crossRustPlatform = mkRustPlatform pkgsCross;
            in
            mkPackage {
              pkgs = pkgsCross;
              rustPlatform = crossRustPlatform;
              target = "aarch64-unknown-linux-musl";
            };
        };

        apps = {
          gh-ghq-cd = flake-utils.lib.mkApp {
            drv = self.packages.${system}.gh-ghq-cd;
          };
          default = self.apps.${system}.gh-ghq-cd;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            (pkgs.rust-bin.stable.latest.default)
            pkgs.cargo-watch
          ];
        };
      }
    );
}
