{
  description = "Habsat flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=staging-next";
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
          stdenv,
          qemu,
          llvmPackages,
          cmake,
          just,
          mold,
          libcamera,
          diesel-cli,
          sqlite,
        }:
        mkShell.override { inherit stdenv; } {
          nativeBuildInputs = [
            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })
            pkg-config
            llvmPackages.libclang
            just
            cmake
            diesel-cli
            sqlite
          ];

          buildInputs = [
          ];

          env = {
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${stdenv.cc.cc}/include/c++/${stdenv.cc.version}/${stdenv.targetPlatform.config} -isystem ${stdenv.cc.cc}/include/c++/${stdenv.cc.version} -isystem ${stdenv.cc.libc.dev}/include";
          };
        }
      ) { };

    };
}
