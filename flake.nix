{
  description = "pass-tomb extension just for macOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        runtimeDeps = [ pkgs.gnupg pkgs.pass ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "pass-tomb-osx";
          version = "1.0.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.makeWrapper ];
          buildInputs = runtimeDeps;

          postFixup = ''
            for bin in pass-tomb pass-open pass-close pass-timer; do
              wrapProgram "$out/bin/$bin" \
                --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps}
            done
          '';

          meta = with pkgs.lib; {
            description = "pass-tomb extension just for macOS";
            platforms = platforms.darwin;
            license = licenses.mit;
          };
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          buildInputs = runtimeDeps;
        };
      });
}
