{
  description = "minimal-rust-project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib =
          (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        cargoToml =
          builtins.fromTOML (builtins.readFile ./Cargo.toml);

        commonArgs = {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          inherit src;
          strictDeps = true;

          nativeBuildInputs = [
            pkgs.pkg-config
          ];

          buildInputs = with pkgs; [
            sqlite
          ];
        };

        cargoArtifacts =
          craneLib.buildDepsOnly commonArgs;

        my-crate =
          craneLib.buildPackage (
            commonArgs // {
              inherit cargoArtifacts;
            }
          );
      in
      {
        checks = {
          inherit my-crate;
        };

        packages = {
          default = my-crate;
        };

        apps.default =
          flake-utils.lib.mkApp {
            drv = my-crate;
          };

        devShells.default =
          craneLib.devShell {
            checks = self.checks.${system};

            packages = with pkgs; [
              rust-analyzer
              cargo-watch
            ];
          };
      }
    );
}
