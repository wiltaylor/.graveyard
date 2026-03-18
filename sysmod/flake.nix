{
  description = "A very basic flake";

  inputs = {
    syspkg.url = "github:wiltaylor/syspkg/master";
  };

  outputs = {self, syspkg }:
  let
    lib = import ./lib { lib = syspkg.lib; self = lib; };
  in {
    sys = lib.mkSystem { modules = []; };
    
  };
}
