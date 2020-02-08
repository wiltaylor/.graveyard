#!/bin/bash

set -e -u

sed -i 's/#\(en_US\.UTF-8\)/\1/' /etc/locale.gen
locale-gen

ln -sf /usr/share/zoneinfo/UTC /etc/localtime

usermod -s /usr/bin/zsh root
cp -aT /etc/skel/ /root/
chmod 700 /root

sed -i 's/#\(PermitRootLogin \).\+/\1yes/' /etc/ssh/sshd_config
sed -i "s/#Server/Server/g" /etc/pacman.d/mirrorlist
sed -i 's/#\(Storage=\)auto/\1volatile/' /etc/systemd/journald.conf

sed -i 's/#\(HandleSuspendKey=\)suspend/\1ignore/' /etc/systemd/logind.conf
sed -i 's/#\(HandleHibernateKey=\)hibernate/\1ignore/' /etc/systemd/logind.conf
sed -i 's/#\(HandleLidSwitch=\)suspend/\1ignore/' /etc/systemd/logind.conf

systemctl enable pacman-init.service choose-mirror.service
systemctl set-default graphical.target

useradd -m -p "" -g users -G "adm,audio,floppy,log,network,rfkill,scanner,storage,optical,power,wheel" -s /usr/bin/zsh arch

rm /usr/share/xsessions/i3.desktop
rm /usr/share/xsessions/openbox-kde.desktop
rm /usr/share/xsessions/i3-with-shmlog.desktop
rm /usr/share/xsessions/openbox.desktop


rm /usr/share/applications/lxqt-hibernate.desktop
rm /usr/share/applications/lxqt-lockscreen.desktop
rm /usr/share/applications/lxqt-logout.desktop
rm /usr/share/applications/lxqt-suspend.desktop


cp /usr/share/color-schemes/BreezeDark.colors /etc/skel/.config/kdeglobals
