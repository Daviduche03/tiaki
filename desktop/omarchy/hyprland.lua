-- Tiaki session on Omarchy chrome, one-to-one.
-- Bootstrap keeps path setup here; Omarchy defaults own everything else.
dofile((os.getenv("OMARCHY_PATH") or "/usr/share/omarchy") .. "/default/hypr/bootstrap.lua")

-- Full Omarchy defaults: bindings, gestures, envs, looknfeel, input,
-- windows, theme overrides, software-rendering fallbacks.
require("default.hypr.omarchy")

-- Tiaki autostart: shell only, plus the software-rendering probe. (Upstream
-- autostart pulls udiskie/herdr/powerprofiles/provisioning; those come back
-- with the image build.) The probe marks llvmpipe/softpipe sessions and
-- reloads so software-rendering.lua drops animations/blur/shadows there.
hl.on("hyprland.start", function()
  hl.exec_cmd("dbus-update-activation-environment --systemd --all")
  hl.exec_cmd("tiaki-launch-shell")
  hl.exec_cmd("omarchy-hyprland-software-rendering")
end)
