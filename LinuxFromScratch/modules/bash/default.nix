{pkgs, config, lib, ...}:
let
  lfs-check = pkgs.writeScriptBin "version-check.sh" ''
    #!${pkgs.bash}/bin/bash
    # Simple script to list version numbers of critical development tools
    export LC_ALL=C
    bash --version | head -n1 | cut -d" " -f2-4
    MYSH=$(readlink -f /bin/sh)
    echo "/bin/sh -> $MYSH"
    echo $MYSH | grep -q bash || echo "ERROR: /bin/sh does not point to bash"
    unset MYSH

    echo -n "Binutils: "; ld --version | head -n1 | cut -d" " -f3-
    bison --version | head -n1
    bzip2 --version 2>&1 < /dev/null | head -n1 | cut -d" " -f1,6-
    echo -n "Coreutils: "; chown --version | head -n1 | cut -d")" -f2
    diff --version | head -n1
    find --version | head -n1
    gawk --version | head -n1
    gcc --version | head -n1
    g++ --version | head -n1
    ldd --version | head -n1 | cut -d" " -f2-  # glibc version
    grep --version | head -n1
    gzip --version | head -n1
    cat /proc/version
    m4 --version | head -n1
    make --version | head -n1
    patch --version | head -n1
    echo Perl `perl -V:version`
    python3 --version
    sed --version | head -n1
    tar --version | head -n1
    makeinfo --version | head -n1  # texinfo version
    xz --version | head -n1

    echo 'int main(){}' > dummy.c && g++ -o dummy dummy.c
    if [ -x dummy ]
      then echo "g++ compilation OK";
      else echo "g++ compilation failed"; fi
    rm -f dummy.c dummy
  '';

  wget-list = pkgs.writeText "wget-list" ''
    http://download.savannah.gnu.org/releases/acl/acl-2.2.53.tar.gz
    http://download.savannah.gnu.org/releases/attr/attr-2.4.48.tar.gz
    http://ftp.gnu.org/gnu/autoconf/autoconf-2.71.tar.xz
    http://ftp.gnu.org/gnu/automake/automake-1.16.3.tar.xz
    http://ftp.gnu.org/gnu/bash/bash-5.1.tar.gz
    https://github.com/gavinhoward/bc/releases/download/3.3.0/bc-3.3.0.tar.xz
    http://ftp.gnu.org/gnu/binutils/binutils-2.36.1.tar.xz
    http://ftp.gnu.org/gnu/bison/bison-3.7.5.tar.xz
    https://www.sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz
    https://github.com/libcheck/check/releases/download/0.15.2/check-0.15.2.tar.gz
    http://ftp.gnu.org/gnu/coreutils/coreutils-8.32.tar.xz
    https://dbus.freedesktop.org/releases/dbus/dbus-1.12.20.tar.gz
    http://ftp.gnu.org/gnu/dejagnu/dejagnu-1.6.2.tar.gz
    http://ftp.gnu.org/gnu/diffutils/diffutils-3.7.tar.xz
    https://downloads.sourceforge.net/project/e2fsprogs/e2fsprogs/v1.46.1/e2fsprogs-1.46.1.tar.gz
    https://sourceware.org/ftp/elfutils/0.183/elfutils-0.183.tar.bz2
    https://dev.gentoo.org/~blueness/eudev/eudev-3.2.10.tar.gz
    https://prdownloads.sourceforge.net/expat/expat-2.2.10.tar.xz
    https://prdownloads.sourceforge.net/expect/expect5.45.4.tar.gz
    http://ftp.astron.com/pub/file/file-5.39.tar.gz
    http://ftp.gnu.org/gnu/findutils/findutils-4.8.0.tar.xz
    https://github.com/westes/flex/releases/download/v2.6.4/flex-2.6.4.tar.gz
    http://ftp.gnu.org/gnu/gawk/gawk-5.1.0.tar.xz
    http://ftp.gnu.org/gnu/gcc/gcc-10.2.0/gcc-10.2.0.tar.xz
    http://ftp.gnu.org/gnu/gdbm/gdbm-1.19.tar.gz
    http://ftp.gnu.org/gnu/gettext/gettext-0.21.tar.xz
    http://ftp.gnu.org/gnu/glibc/glibc-2.33.tar.xz
    http://ftp.gnu.org/gnu/gmp/gmp-6.2.1.tar.xz
    http://ftp.gnu.org/gnu/gperf/gperf-3.1.tar.gz
    http://ftp.gnu.org/gnu/grep/grep-3.6.tar.xz
    http://ftp.gnu.org/gnu/groff/groff-1.22.4.tar.gz
    https://ftp.gnu.org/gnu/grub/grub-2.04.tar.xz
    http://ftp.gnu.org/gnu/gzip/gzip-1.10.tar.xz
    https://github.com/Mic92/iana-etc/releases/download/20210202/iana-etc-20210202.tar.gz
    http://ftp.gnu.org/gnu/inetutils/inetutils-2.0.tar.xz
    https://launchpad.net/intltool/trunk/0.51.0/+download/intltool-0.51.0.tar.gz
    https://www.kernel.org/pub/linux/utils/net/iproute2/iproute2-5.10.0.tar.xz
    https://www.kernel.org/pub/linux/utils/kbd/kbd-2.4.0.tar.xz
    https://www.kernel.org/pub/linux/utils/kernel/kmod/kmod-28.tar.xz
    http://www.greenwoodsoftware.com/less/less-563.tar.gz
    http://www.linuxfromscratch.org/lfs/downloads/10.1/lfs-bootscripts-20210201.tar.xz
    https://www.kernel.org/pub/linux/libs/security/linux-privs/libcap2/libcap-2.48.tar.xz
    https://sourceware.org/pub/libffi/libffi-3.3.tar.gz
    http://download.savannah.gnu.org/releases/libpipeline/libpipeline-1.5.3.tar.gz
    http://ftp.gnu.org/gnu/libtool/libtool-2.4.6.tar.xz
    https://www.kernel.org/pub/linux/kernel/v5.x/linux-5.10.17.tar.xz
    http://ftp.gnu.org/gnu/m4/m4-1.4.18.tar.xz
    http://ftp.gnu.org/gnu/make/make-4.3.tar.gz
    http://download.savannah.gnu.org/releases/man-db/man-db-2.9.4.tar.xz
    https://www.kernel.org/pub/linux/docs/man-pages/man-pages-5.10.tar.xz
    https://github.com/mesonbuild/meson/releases/download/0.57.1/meson-0.57.1.tar.gz
    https://ftp.gnu.org/gnu/mpc/mpc-1.2.1.tar.gz
    http://www.mpfr.org/mpfr-4.1.0/mpfr-4.1.0.tar.xz
    http://ftp.gnu.org/gnu/ncurses/ncurses-6.2.tar.gz
    https://github.com/ninja-build/ninja/archive/v1.10.2/ninja-1.10.2.tar.gz
    https://www.openssl.org/source/openssl-1.1.1j.tar.gz
    http://ftp.gnu.org/gnu/patch/patch-2.7.6.tar.xz
    https://www.cpan.org/src/5.0/perl-5.32.1.tar.xz
    https://pkg-config.freedesktop.org/releases/pkg-config-0.29.2.tar.gz
    https://sourceforge.net/projects/procps-ng/files/Production/procps-ng-3.3.17.tar.xz
    https://sourceforge.net/projects/psmisc/files/psmisc/psmisc-23.4.tar.xz
    https://www.python.org/ftp/python/3.9.2/Python-3.9.2.tar.xz
    https://www.python.org/ftp/python/doc/3.9.2/python-3.9.2-docs-html.tar.bz2
    http://ftp.gnu.org/gnu/readline/readline-8.1.tar.gz
    http://ftp.gnu.org/gnu/sed/sed-4.8.tar.xz
    https://github.com/shadow-maint/shadow/releases/download/4.8.1/shadow-4.8.1.tar.xz
    http://www.infodrom.org/projects/sysklogd/download/sysklogd-1.5.1.tar.gz
    https://github.com/systemd/systemd/archive/v247/systemd-247.tar.gz
    http://anduin.linuxfromscratch.org/LFS/systemd-man-pages-247.tar.xz
    http://download.savannah.gnu.org/releases/sysvinit/sysvinit-2.98.tar.xz
    http://ftp.gnu.org/gnu/tar/tar-1.34.tar.xz
    https://downloads.sourceforge.net/tcl/tcl8.6.11-src.tar.gz
    https://downloads.sourceforge.net/tcl/tcl8.6.11-html.tar.gz
    http://ftp.gnu.org/gnu/texinfo/texinfo-6.7.tar.xz
    https://www.iana.org/time-zones/repository/releases/tzdata2021a.tar.gz
    http://anduin.linuxfromscratch.org/LFS/udev-lfs-20171102.tar.xz
    https://www.kernel.org/pub/linux/utils/util-linux/v2.36/util-linux-2.36.2.tar.xz
    http://anduin.linuxfromscratch.org/LFS/vim-8.2.2433.tar.gz
    https://cpan.metacpan.org/authors/id/T/TO/TODDR/XML-Parser-2.46.tar.gz
    https://tukaani.org/xz/xz-5.2.5.tar.xz
    https://zlib.net/zlib-1.2.11.tar.xz
    https://github.com/facebook/zstd/releases/download/v1.4.8/zstd-1.4.8.tar.gz
    http://www.linuxfromscratch.org/patches/lfs/10.1/bzip2-1.0.8-install_docs-1.patch
    http://www.linuxfromscratch.org/patches/lfs/10.1/coreutils-8.32-i18n-1.patch
    http://www.linuxfromscratch.org/patches/lfs/10.1/glibc-2.33-fhs-1.patch
    http://www.linuxfromscratch.org/patches/lfs/10.1/kbd-2.4.0-backspace-1.patch
    http://www.linuxfromscratch.org/patches/lfs/10.1/sysvinit-2.98-consolidated-1.patch
    http://www.linuxfromscratch.org/patches/lfs/10.1/systemd-247-upstream_fixes-1.patch
  '';

  md5sums = pkgs.writeText "md5sums" ''
    007aabf1dbb550bcddde52a244cd1070  acl-2.2.53.tar.gz
    bc1e5cb5c96d99b24886f1f527d3bb3d  attr-2.4.48.tar.gz
    12cfa1687ffa2606337efe1a64416106  autoconf-2.71.tar.xz
    c27f608a4e1f302ec7ce42f1251c184e  automake-1.16.3.tar.xz
    bb91a17fd6c9032c26d0b2b78b50aff5  bash-5.1.tar.gz
    452ae2d467b1d7212bb7896c0c689825  bc-3.3.0.tar.xz
    628d490d976d8957279bbbff06cf29d4  binutils-2.36.1.tar.xz
    9b762dc24a6723f86d14d957d3deeb90  bison-3.7.5.tar.xz
    67e051268d0c475ea773822f7500d0e5  bzip2-1.0.8.tar.gz
    50fcafcecde5a380415b12e9c574e0b2  check-0.15.2.tar.gz
    022042695b7d5bcf1a93559a9735e668  coreutils-8.32.tar.xz
    e1b07516533f351b3aba3423fafeffd6  dejagnu-1.6.2.tar.gz
    4824adc0e95dbbf11dfbdfaad6a1e461  diffutils-3.7.tar.xz
    8c52585522b7ca6bdae2bdecba27b3a4  e2fsprogs-1.46.1.tar.gz
    6f58aa1b9af1a5681b1cbf63e0da2d67  elfutils-0.183.tar.bz2
    60b135a189523f333cea5f71a3345c8d  eudev-3.2.10.tar.gz
    e0fe49a6b3480827c9455e4cfc799133  expat-2.2.10.tar.xz
    00fce8de158422f5ccd2666512329bd2  expect5.45.4.tar.gz
    1c450306053622803a25647d88f80f25  file-5.39.tar.gz
    eeefe2e6380931a77dfa6d9350b43186  findutils-4.8.0.tar.xz
    2882e3179748cc9f9c23ec593d6adc8d  flex-2.6.4.tar.gz
    8470c34eeecc41c1aa0c5d89e630df50  gawk-5.1.0.tar.xz
    e9fd9b1789155ad09bcf3ae747596b50  gcc-10.2.0.tar.xz
    aeb29c6a90350a4c959cd1df38cd0a7e  gdbm-1.19.tar.gz
    40996bbaf7d1356d3c22e33a8b255b31  gettext-0.21.tar.xz
    390bbd889c7e8e8a7041564cb6b27cca  glibc-2.33.tar.xz
    0b82665c4a92fd2ade7440c13fcaa42b  gmp-6.2.1.tar.xz
    9e251c0a618ad0824b51117d5d9db87e  gperf-3.1.tar.gz
    f47fe27049510b2249dba7f862ac1b51  grep-3.6.tar.xz
    08fb04335e2f5e73f23ea4c3adbf0c5f  groff-1.22.4.tar.gz
    5aaca6713b47ca2456d8324a58755ac7  grub-2.04.tar.xz
    691b1221694c3394f1c537df4eee39d3  gzip-1.10.tar.xz
    1c193a4d6ca36274570d1505140a7bee  iana-etc-20210202.tar.gz
    5e1018502cd131ed8e42339f6b5c98aa  inetutils-2.0.tar.xz
    12e517cac2b57a0121cda351570f1e63  intltool-0.51.0.tar.gz
    19ffea480a21e600453776b7225f3319  iproute2-5.10.0.tar.xz
    3cac5be0096fcf7b32dcbd3c53831380  kbd-2.4.0.tar.xz
    0a2b887b1b3dfb8c0b3f41f598203e56  kmod-28.tar.xz
    1ee44fa71447a845f6eef5b3f38d2781  less-563.tar.gz
    1fc441ef96c522974f7267dec8b40a47  lfs-bootscripts-20210201.tar.xz
    ca71693a9abe4e0ad9cc33a755ee47e0  libcap-2.48.tar.xz
    6313289e32f1d38a9df4770b014a2ca7  libffi-3.3.tar.gz
    dad443d0911cf9f0f1bd90a334bc9004  libpipeline-1.5.3.tar.gz
    1bfb9b923f2c1339b4d2ce1807064aa5  libtool-2.4.6.tar.xz
    4908707ed841923d8d1814130d5c380f  linux-5.10.17.tar.xz
    730bb15d96fffe47e148d1e09235af82  m4-1.4.18.tar.xz
    fc7a67ea86ace13195b0bce683fd4469  make-4.3.tar.gz
    6e233a555f7b9ae91ce7cd0faa322bce  man-db-2.9.4.tar.xz
    4ae3f74a1beddd919936e1058642644c  man-pages-5.10.tar.xz
    fbd744560351491892478a36a1586815  meson-0.57.1.tar.gz
    9f16c976c25bb0f76b50be749cd7a3a8  mpc-1.2.1.tar.gz
    bdd3d5efba9c17da8d83a35ec552baef  mpfr-4.1.0.tar.xz
    e812da327b1c2214ac1aed440ea3ae8d  ncurses-6.2.tar.gz
    639f75bc2e3b19ab893eaf2c810d4eb4  ninja-1.10.2.tar.gz
    cccaa064ed860a2b4d1303811bf5c682  openssl-1.1.1j.tar.gz
    78ad9937e4caadcba1526ef1853730d5  patch-2.7.6.tar.xz
    7f104064b906ad8c7329ca5e409a32d7  perl-5.32.1.tar.xz
    f6e931e319531b736fadc017f470e68a  pkg-config-0.29.2.tar.gz
    d60613e88c2f442ebd462b5a75313d56  procps-ng-3.3.17.tar.xz
    8114cd4489b95308efe2509c3a406bbf  psmisc-23.4.tar.xz
    f0dc9000312abeb16de4eccce9a870ab  Python-3.9.2.tar.xz
    719cd64a4c5768b646b716df20229400  python-3.9.2-docs-html.tar.bz2
    e9557dd5b1409f5d7b37ef717c64518e  readline-8.1.tar.gz
    6d906edfdb3202304059233f51f9a71d  sed-4.8.tar.xz
    4b05eff8a427cf50e615bda324b5bc45  shadow-4.8.1.tar.xz
    c70599ab0d037fde724f7210c2c8d7f8  sysklogd-1.5.1.tar.gz
    e3254f7622ea5cf2322b1b386a98ba59  sysvinit-2.98.tar.xz
    9a08d29a9ac4727130b5708347c0f5cf  tar-1.34.tar.xz
    8a4c004f48984a03a7747e9ba06e4da4  tcl8.6.11-src.tar.gz
    e358a9140c3a171e42f18c8a7f6a36ea  tcl8.6.11-html.tar.gz
    d4c5d8cc84438c5993ec5163a59522a6  texinfo-6.7.tar.xz
    20eae7d1da671c6eac56339c8df85bbd  tzdata2021a.tar.gz
    27cd82f9a61422e186b9d6759ddf1634  udev-lfs-20171102.tar.xz
    f78419af679ac9678190ad961eb3cf27  util-linux-2.36.2.tar.xz
    a26555c8919cf40938d2428d834bf913  vim-8.2.2433.tar.gz
    80bb18a8e6240fcf7ec2f7b57601c170  XML-Parser-2.46.tar.gz
    aa1621ec7013a19abab52a8aff04fe5b  xz-5.2.5.tar.xz
    85adef240c5f370b308da8c938951a68  zlib-1.2.11.tar.xz
    e873db7cfa5ef05832e6d55a5a572840  zstd-1.4.8.tar.gz
    6a5ac7e89b791aae556de0f745916f7f  bzip2-1.0.8-install_docs-1.patch
    cd8ebed2a67fff2e231026df91af6776  coreutils-8.32-i18n-1.patch
    9a5997c3452909b1769918c759eff8a2  glibc-2.33-fhs-1.patch
    f75cca16a38da6caa7d52151f7136895  kbd-2.4.0-backspace-1.patch
    4900322141d493e74020c9cf437b2cdc  sysvinit-2.98-consolidated-1.patch
  '';
in {

  environment.systemPackages = with pkgs; [
    lfs-check
    bash
    binutils
    bison
    bzip2
    coreutils
    diffutils
    findutils
    gawk
    gcc
    glibc
    gnugrep
    gzip
    gnum4
    gnumake
    gnupatch
    perl
    python3Full
    gnused
    gnutar
    texinfo
    xz
  ];

  programs.bash.interactiveShellInit = ''
    ${pkgs.figlet}/bin/figlet LFS 10.1

    echo "Run version-check.sh to verify dependancies"
    echo ""
    echo "md5sums and wget-list have been copied to home"

    cp ${md5sums} ~/md5sums
    cp ${wget-list} ~/wget-list
  '';
}
