{
  description = "Habsat flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      host_system = "x86_64-linux";

      pkgs = import nixpkgs {
        system = host_system;
        overlays = [ (import rust-overlay) ];
      };

      pkgsCross = pkgs.pkgsCross.aarch64-multiplatform;
      rust-bin = rust-overlay.lib.mkRustBin { } pkgsCross.buildPackages;
    in
    {

      devShells.${host_system}.default = pkgsCross.callPackage (
        {
          mkShell,
          pkg-config,
          qemu,
          stdenv,
          glibc,
        }:
        mkShell {
          nativeBuildInputs = [
            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })
            pkg-config
          ];

          depsBuildBuild = [ qemu ];

          buildInputs = [ glibc.static ];

          env = {
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER = "${stdenv.cc.targetPrefix}cc";
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER = "qemu-aarch64";
          };
        }
      ) { };

    };
}
