{ pkgs ? <nixpkgs> }:
let
  tmuxIde = pkgs.writeScriptBin "tmuxide" ''
    tmux new-session -d -s dev vim
    tmux rename-window 'neovim'
    tmux select-window -t 'dev:0'
    tmux split-window -v -p 30 zsh
    tmux attach-session -t dev
  '';
in pkgs.mkShell {
  name = "golangdevshell";
  buildInputs = with pkgs; [
    go
    dep2nix
    delve
    tmuxIde
  ];

  shellHook = ''
    echo "DEV DevShell"
    export SHELL=zsh
    export EDITOR=vim
  '';
}
