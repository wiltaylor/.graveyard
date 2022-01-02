{pkgs, ...}:
{
  mkWks = {
    name, 
    packages ? [], 
    guiScript ? "echo no gui", 
    homeIsolation ? false, 
    shellScript ? "",
    startHook ? "",
    system, 
    shell ? "zsh"}:
    let
      nixwksScript = pkgs.writeShellApplication {
      name = "nixwks";
      runtimeInputs = packages;

      text = ''
        PROFILE_PATH="$HOME/.local/share/nixwks/${name}"
        ISOHOME_PATH="$HOME/.local/share/nixwks/home/${name}"

        case "$1" in
        "shell")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$ISOHOME_PATH"
            ln -sf "$HOME" "$ISOHOME_PATH/actual_home"

            [ -n "${"$"}{REALHOME-}" ] || export REALHOME="$HOME"
            export HOME="$ISOHOME_PATH"
          '' else ""}

          ${startHook}

          exec ${shell}
        ;;
        "gui")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$ISOHOME_PATH"
            ln -sf "$HOME" "$ISOHOME_PATH/actual_home"

            [ -n "${"$"}{REALHOME-}" ] || export REALHOME="$HOME"
            export HOME="$ISOHOME_PATH"

          '' else ""}

          ${startHook}

          ${guiScript}
        ;;
        "run")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$ISOHOME_PATH"
            ln -sf "$HOME" "$ISOHOME_PATH/actual_home"

            [ -n "${"$"}{REALHOME-}" ] || export REALHOME="$HOME"
            export HOME="$ISOHOME_PATH"
          '' else ""}

            shift 1

          ${startHook}

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
}
