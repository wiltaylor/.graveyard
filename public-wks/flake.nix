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
      office = mkWks {
        name = "office";
        system = sys;
        homeIsolation = true;
        packages = with pkgs; [ wpsoffice foxitreader onlyoffice-bin ];
        guiScript = ''
          OPT=$(echo -e "WPS Office\nOnly Office\nFoxitReader" | rofi -dmenu)
          case $OPT in
          "WPS Office")
            exec wps
          ;;
          "Only Office")
            exec onlyoffice-desktopeditors
          ;;
          "FoxitReader")
            exec FoxitReader
          ;;
          *)
          ;;
          esac
        '';
      };

      orgSys = mkWks {
        name = "orgSys";
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
