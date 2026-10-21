{
  description = "A lightweight daemon for adding UI interaction sounds on input events";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {self, ...} @ inputs: let
    supportedSystems = [
      "x86_64-linux"
      "x86_64-darwin"
      "aarch64-linux"
    ];
    forEachSupportedSystem = f:
      inputs.nixpkgs.lib.genAttrs supportedSystems (
        system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [
                inputs.self.overlays.default
              ];
            };
          }
      );
  in {
    overlays.default = final: prev: {
      rustToolchain = with inputs.fenix.packages.${prev.stdenv.hostPlatform.system};
        combine (
          with stable; [
            clippy
            rustc
            cargo
            rustfmt
            rust-src
          ]
        );
    };

    devShells = forEachSupportedSystem (
      {
        pkgs,
        system,
      }: let
        runtimeLibs = with pkgs;
          [
            openssl
          ]
          ++ lib.optionalString stdenv.hostPlatform.isLinux [
            alsa-lib
            systemd
            libinput
          ];
      in {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = runtimeLibs;

          packages = with pkgs; [
            rustToolchain
            openssl
            pkg-config
            cargo-deny
            cargo-edit
            cargo-watch
            rust-analyzer

            self.formatter.${system}
          ];

          env = {
            # Required by rust-analyzer
            RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
            LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
              pkgs.lib.makeLibraryPath runtimeLibs
            );
          };
        };
      }
    );

    packages = forEachSupportedSystem (
      {pkgs, ...}: let
        eventsfx = let
          rustPlatform = pkgs.makeRustPlatform {
            cargo = pkgs.rustToolchain;
            rustc = pkgs.rustToolchain;
          };
        in
          rustPlatform.buildRustPackage {
            pname = "eventsfx";
            version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
            src = self;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            nativeBuildInputs = with pkgs;
              [
                pkg-config
              ]
              ++ lib.optional stdenv.hostPlatform.isLinux [
                autoPatchelfHook
              ];

            buildInputs = with pkgs; [
              alsa-lib
              libinput
              systemd
            ];

            postInstall = ''
              mkdir -p $out/share/eventsfx/audio
              install -Dvm 644 $src/audio/* -t $out/share/eventsfx/audio/

              if [ ! -d "$out/share/eventsfx/audio" ] || [ -z "$(ls -A $out/share/eventsfx/audio)" ]; then
                echo "Failed to copy audio files or directory is empty"
                exit 1
              fi
            '';

            meta = {
              description = "A lightweight daemon for adding UI interaction sounds on input events";
              license = pkgs.lib.licenses.mit;
              mainProgram = "eventsfx";
            };
          };
      in {
        default = eventsfx;
        inherit eventsfx;
      }
    );

    formatter = forEachSupportedSystem ({pkgs, ...}: pkgs.alejandra);
  };
}
