{
  description = "A very basic flake";
  
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs}:
  with import nixpkgs { system = "x86_64-linux"; };
  let
    system = "x86_64-linux";


  in {
    defaultPackage.${system} = import ./default.nix { inherit pkgs; };
    devShell.${system} = import ./shell.nix { inherit pkgs; };
  };
}
