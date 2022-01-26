{
  description = "Flake for building and working with my personal dev utility.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: let

    allPkgs = lib.mkPkgs { 
      inherit nixpkgs; 
      cfg = { allowUnfree = true; };
    };

    lib = import ./lib;

  in {

    devShell = lib.withDefaultSystems (sys: let 
      pkgs = allPkgs."${sys}";
    in import ./shell.nix { inherit pkgs;});

    overlay = top: last: {
        dev = self.packages."${top.system}".dev;
    };

    defaultPackage = lib.withDefaultSystems (sys: self.packages."${sys}".dev);

    packages = lib.withDefaultSystems (sys: let
      pkgs = allPkgs."${sys}";
    in {
      dev = pkgs.buildGoModule rec {
        pname ="dev";
        version = "0.1.0";

        buildInputs = with pkgs; [ ];

        proxyVendor = true;

        src = ./.;

        vendorSha256 = "sha256-wbKJQInRfFYqxCZC+M4mqo5R5LXxuatD0Yzad1O6iGs=";
      };

    });
  };
}
