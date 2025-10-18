{
  description = "VMLab";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
  let
    pkgs = import nixpkgs {
      system = "x86_64-linux";
      config = { allowUnfree = "true";};
    };

  in rec {
    devShell.x86_64-linux = import ./shell.nix { inherit pkgs; };
    defaultPackage.x86_64-linux = packages.x86_64-linux.vmlab;
    defaultApp = apps.vmlab;

    overlay = (self: super: {
      vmlab = packages.x86_64-linux.vmlab;
    });

    apps = {
      vmlab = {
        type = "app";
        program = "${defaultPackage.x86_64-linux}/bin/vmlab";
      };
    };

    packages.x86_64-linux.vmlab = pkgs.buildGoModule rec {
      name = "vmlab";
      version = "0.1.0";

      buildInputs = with pkgs; [];

      src = builtins.path { path = ./.; name = "vmlab"; };

      vendorSha256 = "sha256-pQpattmS9VmO3ZIQUFn66az8GSmB4IvYhTTCFn6SUmo=";
    };
  };
}
