{ pkgs ? <nixpkgs> }:
pkgs.mkShell {
  name = "golangdevshell";
  buildInputs = with pkgs; [
    go
  ];

  shellHook = ''
    echo "KN DevShell"
    export ZKDIR=$(pwd)/.zk
    export EDITOR=vim
    mkdir $ZKDIR -p
  '';
}
