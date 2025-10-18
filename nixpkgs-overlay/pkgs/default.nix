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

pkgs:
with pkgs;
{
  #List all packages you want to put in the package overlay in here.

  g810-led = callPackage ./g810-led {};
  i3blocks-contrib = callPackage ./i3blocks-contrib {};
  emacs-sqlite = pkgs.emacs.override { withSQLite3 = true; };
  jetbrains-toolbox = callPackage ./jetbrains-toolbox {};
}
