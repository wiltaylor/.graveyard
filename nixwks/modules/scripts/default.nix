{pkgs, lib, config, ...}:
with pkgs;
with lib;
with builtins;
let
  cfg = config.wks;
  wksCli = writeShellApplication {
    name = "wks";

    text = ''
      PROFILE_ROOT="$HOME/.local/share/nixwks/"
      VERSION="0.1.0"

      Usage() {
        echo "NIX Workspace Usage:"
        echo "wks {command}"
        echo ""
        echo "Commands:"
        echo "install {flake path} {workspace name} - Installs a workspace"
        echo "uninstall {workspace name} - Removes workspace"
        echo "clean {workspace name} - Cleans up old versions of a workspace"
        echo "shell {workspace name} - Opens a shell in the target workspace"
        echo "gui {workspace name} - Opens the gui shell assigned to  the target workspace"
        echo "run {workspace name} {command} - Runs the target command in the workspace."
        echo "update {workspace name} - Updates the target workspace"
        echo "update-all - Updates all workspaces"
        echo "version - Prints the version of this script"
        echo "help - Prints this message"
      }

      install() {
        if [[ $# -ne 2 ]]; then
          echo "Expected 2 parameters! FlakePath and WorkspaceName!"
          exit 5
        fi

        #This will update the flake cache but fail to create the lock file.
        # shellcheck disable=SC2091
        $(nix flake update "$1" 2> /dev/null) || true

        nix build "$1#$2" --profile "$PROFILE_ROOT/$2"

        mkdir -p "$PROFILE_ROOT/.updates"
        echo "nix build \"$1#$2\" --profile \"$PROFILE_ROOT/$2\"" > "$PROFILE_ROOT/.updates/$2.sh"
        chmod +x "$PROFILE_ROOT/.updates/$2.sh"
      }

      uninstall() {
        if [[ $# -ne 1 ]]; then
          echo "Expected 1 parameter WorkspaceName!"
          exit 5
        fi

        # Hack: need to revisit this
        # shellcheck disable=SC2086
        # shellcheck disable=SC2115
        rm -fr $PROFILE_ROOT/$1*
        rm "$PROFILE_ROOT/.updates/$1.sh"
      }

      list() {
        # HACK: Clean this up later.
        # shellcheck disable=SC2010
        ls --width=1 --color=no "$PROFILE_ROOT" | grep -v home | grep -v "\-link"
      }

      shell() {
        if [[ $# -ne 1 ]]; then
          echo "Expected 1 parameters WorkspaceName!"
          exit 5
        fi

        exec "$PROFILE_ROOT/$1/bin/nixwks" shell
      }

      gui() {
        if [[ $# -ne 1 ]]; then
          echo "Expected 2 parameters WorkspaceName!"
          exit 5
        fi

        exec "$PROFILE_ROOT/$1/bin/nixwks" gui
      }

      runCmd() {
        WKS=$1
        shift 1

        exec "$PROFILE_ROOT/$WKS/bin/nixwks" run "$@"
      }

      update() {
        if [[ $# -ne 1 ]]; then
          echo "Expected 2 parameters WorkspaceName!"
          exit 5
        fi

        #This will update the flake cache but fail to create the lock file.
        # shellcheck disable=SC2091
        $(nix flake update "$1" 2> /dev/null) || true

        "$PROFILE_ROOT/.updates/$1.sh"
      }

      updateAll() {
        for w in $(wks ls)
        do
          wks update $w
        done

      }

      version() {
        echo "wks: $VERSION"
      }

      if [[ $# -eq 0 ]]; then
        Usage
        exit 0
      fi

      CMD="$1"
      shift 1

      case "$CMD" in
      "install")
        install "$@"
      ;;
      "uninstall")
        uninstall "$@"
      ;;
      "clean")
        clean "$@"
      ;;
      "ls")
        list "$@"
      ;;
      "shell")
        shell "$@"
      ;;
      "gui")
        gui "$@"
      ;;
      "run")
        runCmd "$@"
      ;;
      "update")
        update "$@"
      ;;
      "update-all")
        updateAll
      ;;
      "version")
        version "$@"
      ;;
      *)
        Usage
      ;;
      esac
    '';

  };

in {

  options.wks.wksCli = mkOption {
    type = types.package;
    default = wksCli;
    description = "Wks CLI package";
  };

}
