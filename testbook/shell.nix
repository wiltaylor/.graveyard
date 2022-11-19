{ pkgs ? <nixpkgs> }:
pkgs.mkShell {
  name = "bookshell";
  buildInputs = with pkgs; [
    mdbook
    mdbook-admonish
    mdbook-graphviz
    mdbook-katex
    mdbook-mermaid
    mdbook-pdf
    mdbook-plantuml
    just
    graphviz
    plantuml
  ];

  shellHook = ''
    echo "DEV DevShell"
    export SHELL=zsh
    export EDITOR=zvim
  '';
}
