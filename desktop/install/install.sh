#!/bin/sh
# Tiaki installer: partitions a disk, installs Debian, lays down two
# root generations plus the OS payload, and writes the Limine boot config.
#
# Usage: install.sh <disk> [--arch amd64|arm64] [--hostname NAME]
#
# Layout:
#   <disk>p1  EFI (/boot/efi, vfat)
#   <disk>p2  boot A (ext4, kernels for generation A)
#   <disk>p3  root A (ext4, /)
#   <disk>p4  boot B (ext4, kernels for generation B)
#   <disk>p5  root B (ext4, /)
#   <disk>p6  home (/home, ext4, shared)
#
# Kernels live on per-generation XBOOTLDR partitions beside the roots they
# boot; systemd-boot discovers both slots automatically.
#
# State (/var/lib/tiaki) lives on the booted root, so each generation
# carries its own ledger. /home is shared, like any distro.
#
# This script is tested under QEMU (see docs/specs/boot.md). It shells out
# to debootstrap, parted, and mkfs like any installer; there is no custom
# partitioning engine to maintain.
set -eu

# Run from the repository root: the script reads the staged payload/ tree
# (see `make payload`).
for required in payload/agentd payload/sched-launch payload/tiaki-firstboot payload/tiaki-notify payload/desktop.json payload/tiaki-agent payload/tiaki-default-agent; do
  if [ ! -e "$required" ]; then
    echo "missing $required — run from the repo root after \`make payload\`" >&2
    exit 1
  fi
done

DISK="${1:?usage: install.sh <disk> [--arch amd64|arm64] [--hostname NAME]}"
ARCH="amd64"
HOSTNAME="tiaki"
shift
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --hostname) HOSTNAME="$2"; shift 2 ;;
    *) echo "unknown flag: $1" >&2; exit 1 ;;
  esac
done

SUITE="trixie"
MNT="$(mktemp -d /tmp/tiaki-install.XXXXXX)"
cleanup() { umount -R "$MNT" 2>/dev/null || true; rmdir "$MNT" 2>/dev/null || true; }
trap cleanup EXIT

echo "== partitioning $DISK (EFI + boot-A + root-A + boot-B + root-B + home)"
parted -s "$DISK" mklabel gpt
parted -s "$DISK" mkpart ESP fat32 1MiB 513MiB
parted -s "$DISK" set 1 esp on
parted -s "$DISK" mkpart boota ext4 513MiB 1537MiB
parted -s "$DISK" set 2 bls_boot on
parted -s "$DISK" mkpart roota ext4 1537MiB 10241MiB
parted -s "$DISK" mkpart bootb ext4 10241MiB 11265MiB
parted -s "$DISK" set 4 bls_boot on
parted -s "$DISK" mkpart rootb ext4 11265MiB 19969MiB
parted -s "$DISK" mkpart home ext4 19969MiB 100%
case "$DISK" in *[0-9]) P="${DISK}p" ;; *) P="${DISK}" ;; esac
partprobe "$DISK" 2>/dev/null || blockdev --rereadpt "$DISK" 2>/dev/null || sleep 2
# Partition nodes arrive asynchronously via udev after the re-read; wait
# for them instead of asserting immediately, then fail loudly.
tries=0
while [ "$tries" -lt 15 ]; do
  missing=0
  for part in "${P}1" "${P}2" "${P}3" "${P}4" "${P}5" "${P}6"; do
    [ -b "$part" ] || missing=1
  done
  [ "$missing" -eq 0 ] && break
  tries=$((tries + 1))
  sleep 2
done
if [ "$missing" -ne 0 ]; then
  echo "partition nodes never appeared under $DISK; aborting" >&2
  exit 1
fi
mkfs.vfat -n EFI "${P}1" >/dev/null
mkfs.ext4 -q -L tiaki-boot-a "${P}2"
mkfs.ext4 -q -L tiaki-a "${P}3"
mkfs.ext4 -q -L tiaki-boot-b "${P}4"
mkfs.ext4 -q -L tiaki-b "${P}5"
mkfs.ext4 -q -L tiaki-home "${P}6"

echo "== installing Debian $SUITE ($ARCH) into generation A"
mount "${P}3" "$MNT"
mkdir -p "$MNT/boot"
mount "${P}2" "$MNT/boot"
debootstrap --arch "$ARCH" "$SUITE" "$MNT" https://deb.debian.org/debian
mount "${P}6" "$MNT/home"

echo "== base system config"
echo "$HOSTNAME" > "$MNT/etc/hostname"

# Debian is the base; the OS is Tiaki. This is what Settings > About,
# fastfetch, and every /etc/os-release reader will report.
mkdir -p "$MNT/usr/lib"
cat > "$MNT/usr/lib/os-release" <<EOF
PRETTY_NAME="Tiaki"
NAME="Tiaki"
ID=tiaki
ID_LIKE=debian
VERSION="0.1 (dev)"
VERSION_ID="0.1"
HOME_URL="https://github.com/Daviduche03/tiaki"
BUG_REPORT_URL="https://github.com/Daviduche03/tiaki/issues"
EOF
ln -sf ../usr/lib/os-release "$MNT/etc/os-release"
cat > "$MNT/etc/hosts" <<EOF
127.0.0.1 localhost
127.0.1.1 $HOSTNAME
::1 localhost ip6-localhost ip6-loopback
ff02::1 ip6-allnodes
ff02::2 ip6-allrouters
EOF
cat > "$MNT/etc/fstab" <<EOF
LABEL=tiaki-a  /          ext4  defaults  0 1
LABEL=tiaki-boot-a /boot  ext4  defaults  0 2
LABEL=tiaki-home /home    ext4  defaults  0 2
EOF
mkdir -p "$MNT/boot/efi"
mount "${P}1" "$MNT/boot/efi"

echo "== kernel, bootloader, desktop stack"
chroot "$MNT" apt-get update
chroot "$MNT" env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
  "linux-image-$ARCH" systemd systemd-sysv systemd-boot mtools dosfstools \
  sway swaybg foot mousepad dbus openssh-server isc-dhcp-client \
  libxcb-cursor0 libxcb-image0 libxcb-render-util0 libxcb-util1 \
  libgtk-3-0 libgtk-layer-shell0 wayvnc grim curl unzip wtype wl-clipboard brightnessctl pamixer \
  fonts-jetbrains-mono adwaita-icon-theme librsvg2-common git sudo

# The shell is typeset in IBM Plex Sans/Mono, which Debian does not package.
# Same pinned releases as desktop/wayland/Dockerfile.
echo "== IBM Plex (shell typeface)"
chroot "$MNT" sh -c '
  set -eu
  tmp="$(mktemp -d)"
  curl -fsSL "https://github.com/IBM/plex/releases/download/%40ibm/plex-sans%401.1.0/ibm-plex-sans.zip" -o "$tmp/sans.zip"
  curl -fsSL "https://github.com/IBM/plex/releases/download/%40ibm/plex-mono%402.5.0/ibm-plex-mono.zip" -o "$tmp/mono.zip"
  mkdir -p /usr/share/fonts/truetype/ibm-plex
  unzip -j -o "$tmp/sans.zip" "ibm-plex-sans/fonts/complete/ttf/*.ttf" -d /usr/share/fonts/truetype/ibm-plex >/dev/null
  unzip -j -o "$tmp/mono.zip" "ibm-plex-mono/fonts/complete/ttf/*.ttf" -d /usr/share/fonts/truetype/ibm-plex >/dev/null
  rm -rf "$tmp"
  fc-cache -f >/dev/null
  fc-match "IBM Plex Sans" | grep -qi plex
'

echo "== users and network"
chroot "$MNT" useradd -m -s /bin/bash -G sudo,video,render desktop
chroot "$MNT" useradd -m -s /bin/bash -G sudo tiaki || true
if [ -n "${INSTALL_SSH_KEY:-}" ]; then
  mkdir -p "$MNT/home/tiaki/.ssh"
  printf '%s\n' "$INSTALL_SSH_KEY" > "$MNT/home/tiaki/.ssh/authorized_keys"
  chmod 700 "$MNT/home/tiaki/.ssh"
  chmod 600 "$MNT/home/tiaki/.ssh/authorized_keys"
  chroot "$MNT" chown -R tiaki:tiaki /home/tiaki/.ssh || true
fi
mkdir -p "$MNT/etc/systemd/network"
cat > "$MNT/etc/systemd/network/20-wired.network" <<EOF
[Match]
Name=en*
[Network]
DHCP=yes
EOF

echo "== cloning generation A -> B (bit-identical siblings)"
mkdir -p /tmp/tiaki-b /tmp/tiaki-boot-b
mount "${P}5" /tmp/tiaki-b
mount "${P}4" /tmp/tiaki-boot-b
(cd "$MNT" && tar cf - --exclude ./boot --exclude ./home .) | (cd /tmp/tiaki-b && tar xf -)
mkdir -p /tmp/tiaki-b/boot /tmp/tiaki-b/home
(cd "$MNT/boot" && tar cf - --exclude ./efi .) | (cd /tmp/tiaki-boot-b && tar xf -)
sed -i 's/LABEL=tiaki-a/LABEL=tiaki-b/; s/LABEL=tiaki-boot-a/LABEL=tiaki-boot-b/' /tmp/tiaki-b/etc/fstab

echo "== OS payload (binaries, units, tree, wallpapers)"
# Built artifacts are copied from the build host. See Makefile target
# `payload` for the exact list; keep it in sync with desktop/wayland/Dockerfile.
for gen in "$MNT" /tmp/tiaki-b; do
  mkdir -p "$gen/usr/local/bin" "$gen/usr/share/tiaki" "$gen/var/lib/tiaki"
  cp payload/agentd payload/tiaki-shell payload/tiaki-wallpaper payload/tiaki-firstboot payload/tiaki-session payload/sched-launch payload/tiaki-notify "$gen/usr/local/bin/" 2>/dev/null || cp payload/agentd payload/tiaki-shell payload/tiaki-wallpaper payload/tiaki-firstboot payload/sched-launch payload/tiaki-notify "$gen/usr/local/bin/"
  if [ -e payload/tiaki-wm ]; then cp payload/tiaki-wm "$gen/usr/local/bin/" && chmod 0755 "$gen/usr/local/bin/tiaki-wm"; fi
  if [ -e payload/wm-assets ]; then mkdir -p "$gen/usr/share/tiaki/wm" && cp -r payload/wm-assets "$gen/usr/share/tiaki/wm/assets"; fi
  chmod 0755 "$gen/usr/local/bin/agentd" "$gen/usr/local/bin/tiaki-firstboot" "$gen/usr/local/bin/tiaki-session" "$gen/usr/local/bin/sched-launch" "$gen/usr/local/bin/tiaki-notify"
  # Agent center: compositor-independent default-agent, account, usage, launch scripts.
  for helper in payload/tiaki-agent*; do
    cp "$helper" "$gen/usr/local/bin/$(basename "$helper")"
  done
  chmod 0755 "$gen"/usr/local/bin/tiaki-agent* "$gen/usr/local/bin/tiaki-default-agent" "$gen/usr/local/bin/tiaki-launch-"* "$gen/usr/local/bin/tiaki-cmd-"*
  mkdir -p "$gen/usr/share/tiaki/agent"
  cp payload/agent-lib/collectors.py "$gen/usr/share/tiaki/agent/"
  cp payload/desktop.json "$gen/usr/share/tiaki/"
  cp -r payload/icons payload/wallpapers "$gen/usr/share/tiaki/"
  mkdir -p "$gen/etc/tiaki"
  cp payload/etc-tiaki/sway.conf payload/etc-tiaki/wayvnc.conf payload/etc-tiaki/foot.ini "$gen/etc/tiaki/"
  # Tiaki GTK theme: ink windows for every third-party app.
  for profile in gtk-3.0 gtk-4.0; do
    mkdir -p "$gen/home/desktop/.config/$profile"
  done
  cp payload/gtk/gtk.css "$gen/home/desktop/.config/gtk-3.0/gtk.css"
  cp payload/gtk/settings.ini "$gen/home/desktop/.config/gtk-3.0/settings.ini"
  cp payload/gtk/settings.ini "$gen/home/desktop/.config/gtk-4.0/settings.ini"
  chroot "$gen" chown -R desktop:desktop /home/desktop/.config/gtk-3.0 /home/desktop/.config/gtk-4.0 || true
  # Tiaki's default source-view scheme is dark: replace upstream classic.
  cp payload/gtksourceview/classic.xml "$gen/usr/share/gtksourceview-4/styles/classic.xml"
  cp payload/*.service "$gen/etc/systemd/system/"
  # Enable units with explicit symlinks, never `systemctl enable`: one
  # missing file must not void the rest, and no daemon is reachable here.
  mkdir -p "$gen/etc/systemd/system/sysinit.target.wants" \
           "$gen/etc/systemd/system/multi-user.target.wants" \
           "$gen/etc/systemd/system/graphical.target.wants"
  ln -sf /etc/systemd/system/tiaki-firstboot.service \
    "$gen/etc/systemd/system/sysinit.target.wants/"
  for unit in tiaki-ui-serve.service ssh.service systemd-networkd.service; do
    ln -sf "/etc/systemd/system/$unit" \
      "$gen/etc/systemd/system/multi-user.target.wants/"
  done
  ln -sf /etc/systemd/system/tiaki-session.service \
    "$gen/etc/systemd/system/graphical.target.wants/"
done
echo "== verifying payload and units in both generations"
for gen in "$MNT" /tmp/tiaki-b; do
  for needed in usr/local/bin/agentd usr/local/bin/sched-launch usr/local/bin/tiaki-firstboot usr/local/bin/tiaki-shell usr/local/bin/tiaki-session \
      usr/share/tiaki/desktop.json etc/tiaki/sway.conf \
      etc/systemd/system/multi-user.target.wants/ssh.service \
      etc/systemd/system/multi-user.target.wants/systemd-networkd.service \
      etc/systemd/system/multi-user.target.wants/tiaki-ui-serve.service \
      etc/systemd/system/graphical.target.wants/tiaki-session.service \
      etc/systemd/system/sysinit.target.wants/tiaki-firstboot.service; do
    if [ ! -e "$gen/$needed" ] && [ ! -L "$gen/$needed" ]; then
      echo "missing in $gen: $needed" >&2
      exit 1
    fi
  done
done

echo "== bootloader (systemd-boot discovers the XBOOTLDR slots)"
chroot "$MNT" bootctl install --esp-path=/boot/efi || true
KVER="$(ls "$MNT/boot"/vmlinuz-* | head -1 | sed 's/.*vmlinuz-//')"
write_entry() {
  root="$1"
  dir="$2"
  label="$3"
  mkdir -p "$dir/loader/entries"
  cat > "$dir/loader/entries/tiaki-$root.conf" <<EOF
title   Tiaki (generation $label)
linux   /vmlinuz-$KVER
initrd  /initrd.img-$KVER
options root=LABEL=tiaki-$root rw quiet systemd.show_status=auto console=ttyAMA0,115200 console=ttyS0,115200
EOF
}
write_entry a "$MNT/boot" A
write_entry b "$MNT/boot" B
write_entry a /tmp/tiaki-boot-b A
write_entry b /tmp/tiaki-boot-b B

umount /tmp/tiaki-boot-b
umount /tmp/tiaki-b
umount -R "$MNT"
echo "== installed. Generation A boots by default; B is its identical twin."
echo "   Roll back with the other boot-menu entry, or: agentd generation rollback && reboot"
