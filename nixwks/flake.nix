{
  description = "Nix Workspace Scripts";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: 
  let
    lib = import ./lib;

    allPkgs = lib.mkPkgs { inherit nixpkgs; };
    allmods = lib.evalMods {inherit allPkgs; modules = [./modules];};
  in {
    overlay = top: last: { 
      wksCli = allmods."${top.system}".config.wks.wksCli;
    };

    packages = lib.withDefaultSystems (sys: {
      wksCli = allmods."${sys}".config.wks.wksCli; 
    });

    functions = lib.withDefaultSystems (sys:
      import ./lib/build.nix { pkgs = allPkgs."${sys}"; }
    );
  };
}
