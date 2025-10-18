{pkgs, config, lib, ...}:
{
  users.users.lfsuser= {
    name = "lfsuser";
    isNormalUser = true;
    extraGroups = [ "wheel" "networkmanager" ];
    uid = 1000;
    initialPassword = "P@ssw0rd01";
  };
}
