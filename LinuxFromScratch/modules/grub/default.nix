{ pkgs, config, lib, ...}: {
  boot.loader.grub.splashImage = null;
  boot.loader.grub.configurationName = "LFS 10.1 Install Environment";
}
