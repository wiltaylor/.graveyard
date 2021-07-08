{ modulesPath, ...}:
{

  networking.hostName = "lfshost";
  networking.useDHCP = true;

  # Make this config a iso config
  imports = [ 
    "${modulesPath}/installer/cd-dvd/iso-image.nix" 
    ./user
    ./i3wm
    ./bash
    ./grub
  ];
}
