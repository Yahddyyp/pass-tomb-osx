{
  description = "pass-tomb: keep passwords encrypted inside a macOS DMG";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "pass-tomb";
          version = "1.0.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          meta = with pkgs.lib; {
            description = "pass extension for macOS that keeps passwords encrypted inside a DMG tomb";
            homepage = "https://github.com/Yahddyyp/pass-tomb-osx";
            license = licenses.mit;
            platforms = platforms.darwin;
          };
        };

        defaultPackage = self.packages.${system}.default;
      });
}
