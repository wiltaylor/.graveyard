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

        setupHome() {
          ${if homeIsolation then ''
            mkdir -p "$ISOHOME_PATH/.config"
            ln -sf "$HOME" "$ISOHOME_PATH/actual_home"
            ln -sf "$HOME/.config/systemd" "$ISOHOME_PATH/.config/systemd"
            ln -sf "$HOME/.config/environmentd" "$ISOHOME_PATH/.config/environmentd"
            ln -sf "$HOME/.Xauthority" "$ISOHOME_PATH/.Xauthority"

            [ -n "${"$"}{REALHOME-}" ] || export REALHOME="$HOME"
            export HOME="$ISOHOME_PATH"

            touch "$HOME/.zshrc"
          '' else ""}
        }

        case "$1" in
        "shell")
          export PATH="$PROFILE_PATH/bin:$PATH"
          setupHome

          ${startHook}

          exec ${shell}
        ;;
        "gui")
          export PATH="$PROFILE_PATH/bin:$PATH"
          setupHome

          ${startHook}

          ${guiScript}
        ;;
        "run-exec")
          export PATH="$PROFILE_PATH/bin:$PATH"
          setupHome

          ${startHook}

          exec $1
        ;;
        "run")
          export PATH="$PROFILE_PATH/bin:$PATH"
          setupHome

          shift 1

          ${startHook}

          exec "$@"
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
