{
  description = "A list of my public workspaces";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    wks.url = "github:wiltaylor/nixwks";
  };

  outputs = { self, nixpkgs, wks}: let
    lib = import ./lib;
    allPkgs = lib.mkPkgs { inherit nixpkgs; cfg = { allowUnfree = true;};};

  in {
    packages = lib.withDefaultSystems (sys: let 
      pkgs = allPkgs."${sys}";
      mkWks = wks.functions."${sys}".mkWks;

    in {
      orgSys = mkWks {
        name = "OrgSys";
        system = sys;
        startHook = ''
          ln -sf "$REALHOME/vaults" "$HOME/vaults"
        '';
        homeIsolation = true;
        packages = with pkgs; [ obsidian foxitreader xmind zotero rofi];
        guiScript = ''
          OPT=$(echo -e "Obsidian\nZotero\nXMind\nFoxitReader" | rofi -dmenu)
          case $OPT in
          "Obsidian")
            exec obsidian
          ;;
          "Zotero")
            exec zotero
          ;;
          "FoxitReader")
            exec FoxitReader
          ;;
          "XMind")
            exec XMind
          ;;
          *)
          ;;
          esac
        '';
      };
    });
  };
}
