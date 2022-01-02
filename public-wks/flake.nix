{
  description = "A list of my public workspaces";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    wks.url = "github:wiltaylor/nixwks";
  };

  outputs = { self, nixpkgs, wks}: let
    lib = import ./lib;
    allPkgs = lib.mkPkgs { inherit nixpkgs; };

  in {
    packages = lib.withDefaultSystems (sys: let 
      pkgs = allPkgs."${sys}";
      mkWks = wks.functions."${sys}".mkWks;
    in {
      orgSys = mkWks {
        name = "OrgSys";
        system = sys;
        packages = with pkgs; [ obsidian foxitreader xmind zotero ];
      };
    });
  };
}
