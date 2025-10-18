{
  description = "My Go Application";

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

    overlay = lib.mkOverlays {
      inherit allPkgs;
      overlayFunc = sys: pkgs: (top: last: {
        vmlab = self.packages."${sys}".vmlab;
      });
    };

    defaultPackage = lib.withDefaultSystems (sys: self.packages."${sys}".vmlab);

    packages = lib.withDefaultSystems (sys: let
      pkgs = allPkgs."${sys}";
    in {
      vmlab = pkgs.buildGoModule rec {
        pname ="vmlab";
        version = "0.1.0";

        buildInputs = with pkgs; [ ];

        proxyVendor = true;

        src = ./.;

        vendorSha256 = "";
      };

    });
  };
}
