{ lib, self, ...}:
{
  inherit lib;

  mkSystem = { modules, ... } @args:
  {
    inherit (lib.nixlib.evalModules {
      modules = (import ../system/module-list.nix) ++ modules;
      args = { lib = self; stdenv = lib.stdenv; };
    }) config options;
  };
}
