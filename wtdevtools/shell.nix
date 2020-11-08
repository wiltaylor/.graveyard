{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [ pkgs.python39 ];
  shellHook = ''
    source .venv/bin/activate
  '';
}
