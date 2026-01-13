{
  description = "GitHub CLI extension to fuzzy find and cd to a ghq managed repository";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, naersk }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk' = pkgs.callPackage naersk { };
      in
      {
        packages = {
          gh-ghq-cd = naersk'.buildPackage {
            src = ./.;
            meta = {
              description = "GitHub CLI extension to fuzzy find and cd to a ghq managed repository";
              homepage = "https://github.com/cappyzawa/gh-ghq-cd";
              license = pkgs.lib.licenses.mit;
              mainProgram = "gh-ghq-cd";
            };
          };
          default = self.packages.${system}.gh-ghq-cd;
        };

        apps = {
          gh-ghq-cd = flake-utils.lib.mkApp {
            drv = self.packages.${system}.gh-ghq-cd;
          };
          default = self.apps.${system}.gh-ghq-cd;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            cargo
            rustc
            cargo-watch
          ];
        };
      }
    );
}
