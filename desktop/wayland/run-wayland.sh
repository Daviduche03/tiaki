#!/bin/sh
set -e

export XDG_RUNTIME_DIR=/run/tiaki
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
chown desktop:desktop "$XDG_RUNTIME_DIR" 2>/dev/null || true

export TIAKI_HOME=/var/lib/tiaki
mkdir -p "$TIAKI_HOME"

if [ ! -e "$TIAKI_HOME/state" ]; then
  /usr/local/bin/agentd --principal user:ada ui set /usr/share/tiaki/desktop.json \
    || echo "agentd: could not install the shell tree" >&2
  /usr/local/bin/agentd --principal user:ada apply user/ada/theme/mode dark \
    --intent "Set the desktop theme" || true
fi

# Delegate a cgroup subtree so scheduled launches are enforced by the kernel.
# This needs a writable cgroup2 mount (privileged container or real init). When
# it fails, sched-launch reports "unsupervised" once and apps still start.
if mkdir -p /sys/fs/cgroup/tiaki 2>/dev/null &&
   echo "+cpu +memory +pids +cpuset" > /sys/fs/cgroup/tiaki/cgroup.subtree_control 2>/dev/null; then
  # Children inherit the creator's ownership, so the session user needs write
  # on this one directory only.
  chown desktop:desktop /sys/fs/cgroup/tiaki 2>/dev/null || true
  echo "sched-launch: cgroup subtree delegated at /sys/fs/cgroup/tiaki"
else
  echo "sched-launch: cgroup delegation unavailable; scheduling is advisory" >&2
fi

/usr/local/bin/agentd --principal user:ada ui serve --host 127.0.0.1 --port 8787 &
sleep 1

exec su -s /bin/sh desktop -c '
  export XDG_RUNTIME_DIR=/run/tiaki
  export HOME=/home/desktop
  export WLR_BACKENDS=headless
  export WLR_LIBINPUT_NO_DEVICES=1
  export WLR_RENDERER=pixman
  export GDK_BACKEND=wayland
  export LANG=C.UTF-8
  export LC_ALL=C.UTF-8
  export XDG_CURRENT_DESKTOP=Tiaki
  export XDG_SESSION_TYPE=wayland
  export TIAKI_CGROUP_ROOT=/sys/fs/cgroup/tiaki
  export GTK_THEME=Adwaita:dark
  export XCURSOR_THEME=Adwaita
  export XCURSOR_SIZE=24
  exec dbus-run-session -- sway -c /etc/tiaki/sway.conf
'
