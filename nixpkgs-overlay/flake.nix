# __          ___ _   _______          _
# \ \        / (_) | |__   __|        | |
#  \ \  /\  / / _| |    | | __ _ _   _| | ___  _ __
#   \ \/  \/ / | | |    | |/ _` | | | | |/ _ \| '__|
#    \  /\  /  | | |    | | (_| | |_| | | (_) | |
#     \/  \/   |_|_|    |_|\__,_|\__, |_|\___/|_|
#                                 __/ |
#                                |___/
# Web: https://wil.dev
# Github: https://github.com/wiltaylor
# Contact: web@wiltaylor.dev
# Feel free to use this configuration as you wish.
{
  description = "Wil's Nix Pkg overlay";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { nixpkgs, ... }: 
  let
    lib = import ./lib;
    allPkgs = lib.mkPkgs { inherit nixpkgs; };
  in {
    overlay = lib.mkOverlays { 
      inherit allPkgs; 
      overlayFunc = sys: pkgs: (top: last: import ./pkgs pkgs);
    };
  };
}
