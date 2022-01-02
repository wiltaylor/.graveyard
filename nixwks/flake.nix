{
  description = "Nix Workspace Scripts";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: 
  let
    system = "x86_64-linux";
    runtimeShell = "#!${pkgs.bash}/bin/bash";
    pkgs = import nixpkgs { 
      inherit system;
    };

    mods = pkgs.lib.evalModules {
      modules = [ {
        imports = [ ./modules];
      }];

      specialArgs = { inherit pkgs runtimeShell; };
    };

    cliPkg = mods.config.wks.wksCli;
  in {
    packages."${system}" = {
      wksCli = cliPkg;
    };

    overlay = (self: super: {
      wksCli = cliPkg;
    });

    functions.mkWks = {name, packages ? [], guiScript ? "echo no gui", homeIsolation ? false, shellScript ? "", system, shell ? "zsh"}: let
      nixwksScript = pkgs.writeShellApplication {
        name = "nixwks";
        runtimeInputs = packages;
        

        text = ''
          PROFILE_PATH="$HOME/.local/share/nixwks/${name}"

          case "$1" in
          "shell")
            export PATH="$PROFILE_PATH/bin:$PATH"
            ${if homeIsolation then ''
              mkdir -p "$PROFILE_PATH/home"
              ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
              export HOME="$PROFILE_PATH/home"
            '' else ""}

            exec ${shell}
          ;;
          "gui")
            export PATH="$PROFILE_PATH/bin:$PATH"
            ${if homeIsolation then ''
              mkdir -p "$PROFILE_PATH/home"
              ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
              export HOME="$PROFILE_PATH/home"
            '' else ""}

            ${guiScript}
          ;;
          "run")
            export PATH="$PROFILE_PATH/bin:$PATH"
            ${if homeIsolation then ''
              mkdir -p "$PROFILE_PATH/home"
              ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
              export HOME="$PROFILE_PATH/home"
            '' else ""}

            shift 1

            ${shell} -c "$@"
          ;;         
          *)
            echo "Unexpected command!"
          ;;
          esac
        '';
      };

      in pkgs.symlinkJoin {
        name = "wks-${name}";
        paths = [ nixwksScript] ++ packages;
        postBuild = shellScript;
      };
  };
}
