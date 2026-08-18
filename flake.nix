{
  description = "A Nix flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    systems.url = "github:nix-systems/triplet";

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
      imports = with inputs; [ treefmt-nix.flakeModule ];

      perSystem =
        { pkgs, system, ... }:
        let
          version = "0.0.1";

          # `fenix.packages.${system}.targets.wasm32-unknown-unknown.stable.rust-std`
          # can be combined in here later if this project ever needs a WASM build.
          toolchain = inputs.fenix.packages.${system}.stable.withComponents [
            "cargo"
            "rustc"
            "rustfmt"
            "clippy"
            "rust-src"
          ];

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
          cargoArtifacts = craneLib.buildDepsOnly { src = craneLib.cleanCargoSource ./.; };
        in
        {
          packages.default = pkgs.callPackage ./nix { inherit version craneLib; };

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
              inputs.fenix.packages.${system}.rust-analyzer
            ];
          };

          treefmt.programs = {
            actionlint.enable = true;
            nixfmt.enable = true;
            rustfmt.enable = true;
          };
        };
    };
}
