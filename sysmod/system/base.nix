{lib, config, stdenv, ...}:
with lib;
{

  options = {
    system.build = mkOption {
      default = {};
      type = types.attrs;
    };
  };

  config = {
    system.build.toplevel = stdenv.mkDerivation {
      pname = "base";
      version = "1.0.0";
      dontBuild = true;
      dontUnpack = true;

      installPhase = ''
        echo '${config.test}' > $out
      '';
    };
  };

}
