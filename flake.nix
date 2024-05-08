{
  description = "A crosshair overlay for wlroots compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages = {
          default = self.packages.${system}.wl-crosshair;
          wl-crosshair = pkgs.rustPlatform.buildRustPackage {
            pname = "wl-crosshair";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = with pkgs; [ makeWrapper ];
            postInstall = ''
              mkdir -p $out/share
              cp -rv ${./cursors} $out/share/cursors
              wrapProgram $out/bin/* \
                --set WL_CROSSHAIR_IMAGE_PATH $out/share/cursors/inverse-v.png
            '';
          };
        };

        apps = {
          default = self.apps.${system}.wl-crosshair;
          wl-crosshair = flake-utils.lib.mkApp {
            drv = self.packages.${system}.default;
          };
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
          ];
        };
      });
}
