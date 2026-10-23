{
  description = "Pass tomb extension for macOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      supportedSystems = [ "aarch64-darwin" "x86_64-darwin" ];
    in
    flake-utils.lib.eachSystem supportedSystems (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        version = "1.5.0";

        sources = {
          aarch64-darwin = {
            url = "https://github.com/Yahddyyp/pass-tomb-osx/releases/download/v${version}/pass-tomb-v${version}-aarch64-apple-darwin.tar.gz";
            sha256 = "ca9245e2016e028f561bfdb3b37d72fb523c669aebd85eedc315bb2d68d53d59";
          };
          x86_64-darwin = {
            url = "https://github.com/Yahddyyp/pass-tomb-osx/releases/download/v${version}/pass-tomb-v${version}-x86_64-apple-darwin.tar.gz";
            sha256 = "9aa3e2272506c7b7a09197a7521e91240afeb18fc7bf1b5aa0510981a06da28a";
          };
        };

        srcInfo = sources.${system};

        runtimeDeps = [ pkgs.gnupg pkgs.pass ];
      in
      {
        packages.default = pkgs.stdenv.mkDerivation {
          pname = "pass-tomb-osx";
          inherit version;

          src = pkgs.fetchurl {
            url = srcInfo.url;
            sha256 = srcInfo.sha256;
          };

          sourceRoot = ".";

          nativeBuildInputs = [ pkgs.makeWrapper ];

          installPhase = ''
            runHook preInstall

            mkdir -p $out/bin

            for bin in pass-tomb pass-open pass-close pass-timer; do
              install -m 755 "$bin" "$out/bin/$bin"
            done

            runHook postInstall
          '';

          postFixup = ''
            for bin in pass-tomb pass-open pass-close pass-timer; do
              wrapProgram "$out/bin/$bin" \
                --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps}
            done
          '';

          meta = with pkgs.lib; {
            description = "Pass tomb extension for macOS";
            homepage = "https://github.com/Yahddyyp/pass-tomb-osx";
            platforms = platforms.darwin;
            license = licenses.mit;
            mainProgram = "pass-tomb";
          };
        };

        packages.pass-tomb = self.packages.${system}.default;

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/pass-tomb";
        };

        devShells.default = pkgs.mkShell {
          packages = runtimeDeps ++ [
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
            pkgs.rustfmt
          ];
        };
      }) // {
        overlays.default = final: prev: {
          pass-tomb = self.packages.${final.stdenv.hostPlatform.system}.default;
          pass-tomb-osx = self.packages.${final.stdenv.hostPlatform.system}.default;
        };
      };
}
