{ pkgs ? <nixpkgs> }:
pkgs.mkShell {
  name = "golangdevshell";
  buildInputs = with pkgs; [
    go
  ];

  shellHook = ''
  echo "KN DevShell"
  '';
}
