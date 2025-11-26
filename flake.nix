{
  inputs = {
    flake-compat = {
      url = github:edolstra/flake-compat;
      flake = false;
    };
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, flake-utils, naersk, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) { inherit system; };
        naersk' = pkgs.callPackage naersk { };
      in {
        defaultPackage = naersk'.buildPackage {
          src = ./.;

          nativeBuildInputs = with pkgs; [
            autoPatchelfHook 
          ];
          
          autoPatchelfIgnoreMissingDeps = [
            "libgcc_s.so.1" 
          ];

          runtimeDependencies = with pkgs; [
            libGL
            libxkbcommon
            vulkan-loader
            wayland
            xorg.libX11
            xorg.libXcursor
            xorg.libxcb
            xorg.libXi
          ];
        };
      }
    );
}
