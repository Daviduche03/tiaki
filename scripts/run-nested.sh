#!/bin/bash
set -e

# Ensure UI tree server is reachable
if ! curl -s --connect-timeout 1 http://127.0.0.1:8787/tree.json >/dev/null 2>&1; then
    echo "Starting agentd UI tree server..."
    /usr/local/bin/agentd --principal user:local ui set /usr/share/tiaki/desktop.json || true
    /usr/local/bin/agentd --principal user:local ui serve --host 127.0.0.1 --port 8787 &
    sleep 1
fi

export GTK_THEME=Adwaita:dark
export XCURSOR_THEME=macOS
export XCURSOR_SIZE=28

echo "Launching Tiaki OS nested session window..."
exec sway -c /etc/tiaki/sway.conf
