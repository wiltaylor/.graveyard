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

{pkgs, lib, config, ...}:
with pkgs;
stdenv.mkDerivation {
  pname = "i3blocks-contrib";
  version = "1.6.1";

  src = fetchFromGitHub {
    owner = "vivien";
    repo = "i3blocks-contrib";
    rev = "1105c0bded5830fa6e2b21cdeb8261b4a1bdde5b";
    sha256 = "/kblMnchf/PfwV7MVQPhvsBIRI4S8iVwA5joKyPKnuc=";
  };

  dontBuild = true;

  installPhase = ''
    mkdir $out/share/i3blocks-contrib -p
    cp . -R $out/share/i3blocks-contrib
  '';
  
}
