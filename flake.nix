{
  description = "A Nix flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    systems.url = "github:UnstoppableMango/nix-systems";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = with inputs; [
        systems.flakeModule
        treefmt-nix.flakeModule
      ];

      perSystem =
        {
          inputs',
          pkgs,
          system,
          ...
        }:
        let
          version = "0.0.1";

          # `fenix.packages.${system}.targets.wasm32-unknown-unknown.stable.rust-std`
          # can be combined in here later if this project ever needs a WASM build.
          toolchain = inputs'.fenix.packages.stable.withComponents [
            "cargo"
            "rustc"
            "rustfmt"
            "clippy"
            "rust-src"
          ];

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
          cargoArtifacts = craneLib.buildDepsOnly { src = craneLib.cleanCargoSource ./.; };

          palutil = pkgs.callPackage ./nix { inherit version craneLib; };
        in
        {
          packages.default = palutil;
          packages.palworld-types-codegen = pkgs.callPackage ./nix/palworld-types-codegen.nix { };

          checks.steam-id-cross-check = pkgs.callPackage ./nix/steam-id-check.nix { inherit palutil; };

          checks.palutil-test = craneLib.cargoTest {
            inherit cargoArtifacts version;
            pname = "palutil";
            src = craneLib.cleanCargoSource ./.;
          };

          checks.palutil-clippy = craneLib.cargoClippy {
            inherit cargoArtifacts version;
            pname = "palutil";
            src = craneLib.cleanCargoSource ./.;
            cargoClippyExtraArgs = "-- -D warnings";
          };

          devShells.default = pkgs.mkShellNoCC {
            packages = [
              pkgs.direnv
              pkgs.gnumake
              pkgs.nixfmt
              toolchain
              inputs'.fenix.packages.rust-analyzer
            ];
          };

          treefmt.programs = {
            actionlint.enable = true;
            nixfmt.enable = true;
            rustfmt = {
              enable = true;
              package = toolchain;
            };
          };
        };
    };
}
