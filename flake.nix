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
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "chromash";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            # Add any system dependencies your binary needs
          ];

          # Runtime dependencies
          propagatedBuildInputs = with pkgs; [
            matugen
            hyprpaper
          ];

          # Make sure the binary can find its dependencies
          postInstall = ''
            wrapProgram $out/bin/chromash \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.matugen pkgs.hyprpaper ]}
          '';

          meta = with pkgs.lib; {
            description = "Dynamic theme manager for Hyprland with Material You theming";
            homepage = "https://github.com/yourusername/chromash";
            license = licenses.mit; # Change to your license
            maintainers = [ ];
            platforms = platforms.linux;
          };
        };

        # Development shell
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            pkg-config
            matugen
            hyprpaper
            hyprland
          ];

          shellHook = ''
            echo "Chromash development environment"
            echo "Run 'cargo build' to build"
            echo "Run 'cargo run -- wallpaper <path>' to test"
          '';
        };

        # App for easy running
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/chromash";
        };
      }
    );
}