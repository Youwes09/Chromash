{
  description = "Chromash - Dynamic Theme Manager for Hyprland";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        staticPkgs = pkgs.pkgsStatic;
      in
      {
        packages = {
          # Standard NixOS package
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "chromash";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
            postInstall = ''
              wrapProgram $out/bin/chromash \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.hyprland ]} \
                --set-default HOME "$HOME"
            '';
          };

          # Portable Static binary for Arch
          static = staticPkgs.rustPlatform.buildRustPackage {
            pname = "chromash";
            version = "0.1.0";
            src = ./.;
            cargoLock = { lockFile = ./Cargo.lock; };
            # Static builds often need to ignore certain check phases
            doCheck = false; 
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc cargo rustfmt clippy pkg-config
            matugen hyprpaper hyprland
          ];
        };
      }
    );
}