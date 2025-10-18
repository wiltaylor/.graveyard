{
  description = "A simple flake for building a live environment to install LFS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/21.05";
  };


  outputs = { self, nixpkgs }: 
  let
    system = "x86_64-linux";

    pkgs = import nixpkgs {
      inherit system;
    };

    lib = pkgs.lib;

    cfg = nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {};

      modules = [
        {
          imports = [ ./modules ];
          nixpkgs.pkgs = pkgs;
        }
      ];
    };

  in {

    packages."${system}".LFSLive = cfg.config.system.build.isoImage;
    defaultPackage."${system}" = self.packages."${system}".LFSLive;


    ISOMedia.LFSLive = cfg; 

  };
}
