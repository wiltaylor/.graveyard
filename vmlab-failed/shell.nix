{ pkgs ? <nixpkgs> }:
pkgs.mkShell {
  name = "golangdevshell";
  buildInputs = with pkgs; [
    go
    dep2nix
  ];

  shellHook = ''
    echo "GODev Env"
  '';
}
