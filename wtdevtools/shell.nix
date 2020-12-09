{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [ pkgs.python39  ];
  shellHook = ''
    rm .venv -fr
    python -m venv .venv
    source .venv/bin/activate
    pip install -e .
  '';
}
