# Thanks SrTobi
# This came from https://gist.github.com/SrTobi/ee6104514e2f14a0f1eae201654f2012
{ pkgs, stdenv, fetchurl, ... }:
let
  name = "jetbrains-toolbox";
  version = "1.16.6319";
    #version = "1.18.7455";
  sha256 = "4e6f333839af8330a09d9d6fdcd6a635c9ca1f0ae239d8f268249dbd095ca880";

  jetbrains-toolbox-src = stdenv.mkDerivation {
    name = "${name}-src";

    src = fetchurl {
        url = "https://download.jetbrains.com/toolbox/${name}-${version}.tar.gz";
        inherit sha256;
    };

    installPhase = ''
        #mkdir -p $out/bin
        cp jetbrains-toolbox $out
    '';
  };

  jetbrains-toolbox-f = extraPkgs: pkgs.appimageTools.wrapType2 {
    inherit name;

    src = jetbrains-toolbox-src;

    extraPkgs = pkgs: with pkgs; [
      libcef
    ] ++ extraPkgs;

  };
  makeWithExtraPackages = f: origArgs:
    let origRes = f origArgs;
    in origRes // { withExtraPkgs = newArgs: (f  newArgs); };

in makeWithExtraPackages jetbrains-toolbox-f []
