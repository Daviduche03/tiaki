#!/bin/sh
# Launch the Tiaki desktop as a fullscreen kiosk.
# Install a browser first, for example: apt install chromium
exec chromium --kiosk --noerrdialogs --disable-infobars http://127.0.0.1:8787
