# ArchISO
This is my personal arch iso configuration. It gives you a simple X environment with LXqt and i3 and includes basic tools to do
a simple installation.

You can follow the instructions on https://wiki.archlinux.org/ to do an install.

This iso also includes a firefox browser install.

## Building
To build you need to be running on an Arch based distro (Manjaro, Arch, Endevor etc) and install the following:

```bash
# pacman -Syu archiso
```

Then you can build with the following command:

Note: It must be run as root or it won't work (this is a open ticket with the archiso project).

```bash
$ sudo ./build.sh
```

This will create the iso in out directory.

## Rebuilding
If you want to rebuild the iso you need to delete the following two directories
```bash
sudo rm work out -fr
```

## Copying to USB
Plug the USB in and figure out what its name is with `lsblk`

Use dd to copy contents of iso. (Replace /dev/sdc with the name of your usb drive).

Note: target the drive not a partition on it or it won't work.

```bash
sudo dd if=./out/nameofiso.iso of=/dev/sdc bs=1M
sync
```

Now you can eject the usb and boot off it :).
