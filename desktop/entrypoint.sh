#!/bin/sh
set -e

if [ ! -e "$TIAKI_HOME/state" ]; then
  /usr/local/bin/agentd --principal user:ada ui set /usr/share/tiaki/desktop.json || true
  /usr/local/bin/agentd --principal user:ada apply user/ada/theme/mode dark \
    --intent "Set the desktop theme" || true
  /usr/local/bin/agentd --principal user:ada session capture >/dev/null 2>&1 || true
fi

exec /usr/local/bin/agentd --principal user:ada ui serve --host 0.0.0.0 --port 8787
