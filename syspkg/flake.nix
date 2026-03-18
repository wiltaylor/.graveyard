{
  description = "My personal NIX PKGS repo";

  inputs = {
    # Upstream nixpkgs packages loaded from here
    nixpkgs-master.url = "github:nixos/nixpkgs/master";
    nixpkgs-unstable.url = "github:nixos/nixpkgs/nixos-unstable";
    nixpkgs-20-09.url = "github:nixos/nixpkgs/nixos-20.09";

    #Put all category repositories here
  };

  outputs = inputs:
  let
    system = "x86_64-linux";
  in {

    nixpkgs = {
      master = inputs.nixpkgs-master.legacyPackages."${system}";
      unstable = inputs.nixpkgs-unstable."${system}";
      "20-09" = inputs.nixpkgs-20-09."${system}";
    };

    lib = import ./lib/default.nix { inherit (inputs.nixpkgs-unstable) lib; nixpkgs = inputs.nixpkgs-unstable; inherit system; };

  };
}
