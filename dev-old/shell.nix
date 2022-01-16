{ pkgs ? <nixpkgs> }:
pkgs.mkShell {
  name = "golangdevshell";
  buildInputs = with pkgs; [
    go
    dep2nix
    delve
  ];

  shellHook = ''
    echo "DEV DevShell"
    export ZKDIR=$(pwd)/.zk
    export EDITOR=vim
    mkdir $ZKDIR -p
  '';
}
