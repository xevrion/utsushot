{
  description = "Supersampled Wayland screenshots via a temporary phantom output";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system:
        f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        utsushot = pkgs.rustPlatform.buildRustPackage {
          pname = "utsushot";
          version = "0.0.1";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          # grim is invoked at runtime, so it has to be on PATH rather than
          # just present at build time.
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/utsushot \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.grim pkgs.wl-clipboard pkgs.libnotify ]}
          '';

          meta = with pkgs.lib; {
            description = "Supersampled Wayland screenshots via a temporary phantom output";
            homepage = "https://github.com/xevrion/utsushot";
            license = licenses.gpl3Plus;
            mainProgram = "utsushot";
            platforms = platforms.linux;
          };
        };
        default = utsushot;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            cargo-deny
            grim
            wl-clipboard
            libnotify
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      checks = forAllSystems (pkgs: {
        fmt = pkgs.runCommand "check-fmt" { buildInputs = [ pkgs.cargo pkgs.rustfmt ]; } ''
          cd ${self}
          cargo fmt --all --check
          touch $out
        '';
      });

      formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
    };
}
