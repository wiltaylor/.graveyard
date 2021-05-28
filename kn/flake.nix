{
  description = "KN flake";

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
    devShell.x86_64-linux = import ./shell.nix { inherit pkgs;};

    defaultPackage.x86_64-linux = packages.x86_64-linux.kn;
    defaultApp = apps.kn;

    overlay = (self: super: {
      kn = packages.x86_64-linux.kn;
    });

    apps = {
      kn = {
        type = "app";
        program = "${defaultPackage}/bin/kn";
      };
    };

    packages.x86_64-linux.kn = pkgs.buildGoModule rec {
      name ="kn";
      version = "0.1.0";

      src = ./.;
      
      vendorSha256 = "sha256-IqaIgjLoGC9SbGOHNHWE/r7nWKmHtOsT8U2kuyxAxnU=";
    };
  };
}
