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

        isVarSet() {
          [[ ${!1-x} == x ]] && return 1 || return 0
        }

        case "$1" in
        "shell")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$PROFILE_PATH/home"
            ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
            isVarSet REALHOME || export REALHOME="$HOME"
            export HOME="$PROFILE_PATH/home"
          '' else ""}

          ${startHook}

          exec ${shell}
        ;;
        "gui")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$PROFILE_PATH/home"
            ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
            isVarSet REALHOME || export REALHOME="$HOME"
            export HOME="$PROFILE_PATH/home"
          '' else ""}

          ${startHook}

          ${guiScript}
        ;;
        "run")
          export PATH="$PROFILE_PATH/bin:$PATH"
          ${if homeIsolation then ''
            mkdir -p "$PROFILE_PATH/home"
            ln -sf "$HOME" "$PROFILE_PATH/home/actual_home"
            isVarSet REALHOME || export REALHOME="$HOME"
            export HOME="$PROFILE_PATH/home"
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
