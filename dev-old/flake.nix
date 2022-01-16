{
  description = "Flake for building and working with my personal dev utility.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: let

    pkgs = import nixpkgs {
      system = "x86_64-linux";
      config = { allowUnfree = "true";};
    };

  in rec {

    devShell.x86_64-linux = import ./shell.nix { inherit pkgs;};

    defaultPackage.x86_64-linux = packages.x86_64-linux.dev;
    defaultApp = apps.dev;


    apps = {
      dev = {
        type = "app";
        program = "${defaultPackage}/bin/dev";
      };
    };

    packages.x86_64-linux.kn = pkgs.buildGoModule rec {
      name ="dev";
      version = "0.1.0";

      buildInputs = with pkgs; [ ];

      src = ./.;
      
      vendorSha256 = "sha256-Pi5mS8YToBTgHgvlT5UthrCx5fHXGce9GGw9OtZQPdg=";
    };
  };
}
