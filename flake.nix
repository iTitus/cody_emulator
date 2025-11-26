{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        naersk-lib = pkgs.callPackage naersk { };
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          src = lib.cleanSource (
            lib.sourceFilesBySuffices ./. [
              ".rs"
              ".wgsl"
              ".toml"
              ".lock"
            ]
          );

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
