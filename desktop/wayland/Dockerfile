# syntax=docker/dockerfile:1

# NOTE: rust-toolchain.toml pins stable + rustfmt/clippy, so the builder must
# track stable too. A versioned image (e.g. rust:1.85) would make rustup
# download the entire current stable toolchain on first build. Reproducibility
# comes from Cargo.lock, not the compiler patch version.
FROM rust:slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config libgtk-3-dev libgtk-layer-shell-dev libglib2.0-dev libcairo2-dev \
        libpango1.0-dev libgdk-pixbuf-2.0-dev libatk1.0-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src

# rust-toolchain.toml asks for rustfmt + clippy, which the rust image does not
# ship. Satisfy that here, with retries, so the compile step never has to reach
# the network for a toolchain — Docker DNS here fails intermittently and a
# failed rustup download aborts the whole build.
RUN set -eu; \
    i=0; \
    until rustup component add rustfmt clippy; do \
        i=$((i + 1)); \
        if [ "$i" -ge 5 ]; then echo "rustup component add failed 5 times" >&2; exit 1; fi; \
        echo "rustup component add failed (attempt $i), retrying" >&2; \
        sleep 5; \
    done; \
    rustup component list --installed | grep -q '^rustfmt'

COPY . .

# Cache the cargo registry and every workspace's target directory, so editing
# one source file does not recompile the whole dependency graph. A cache mount
# is not part of the image layer, so the five binaries are copied to /out for
# the final stage to take.
ENV CARGO_NET_RETRY=10 \
    CARGO_HTTP_TIMEOUT=60
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/src/ui/native/target \
    --mount=type=cache,target=/src/ui/shell/target \
    set -eu; \
    chmod +x desktop/wayland/build-payload.sh; \
    ./desktop/wayland/build-payload.sh /out

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        sway swaybg foot mousepad btop thunar epiphany-browser swaylock \
        fonts-jetbrains-mono fonts-font-awesome adwaita-icon-theme \
        grim curl unzip xz-utils wayvnc libgtk-3-0 libgtk-layer-shell0 ca-certificates wtype wl-clipboard brightnessctl pamixer \
        fastfetch imagemagick dbus adduser librsvg2-common python3 git \
    && rm -rf /var/lib/apt/lists/*

# The shell is typeset in IBM Plex Sans/Mono. Debian does not package it, so
# fetch the pinned IBM releases and cache them. Without this the whole UI
# silently falls back to DejaVu Sans.
ARG PLEX_SANS_VERSION=1.1.0
ARG PLEX_MONO_VERSION=2.5.0
RUN set -eu; \
    tmp="$(mktemp -d)"; \
    curl -fsSL "https://github.com/IBM/plex/releases/download/%40ibm/plex-sans%40${PLEX_SANS_VERSION}/ibm-plex-sans.zip" -o "$tmp/sans.zip"; \
    curl -fsSL "https://github.com/IBM/plex/releases/download/%40ibm/plex-mono%40${PLEX_MONO_VERSION}/ibm-plex-mono.zip" -o "$tmp/mono.zip"; \
    mkdir -p /usr/share/fonts/truetype/ibm-plex; \
    unzip -j -o "$tmp/sans.zip" "ibm-plex-sans/fonts/complete/ttf/*.ttf" -d /usr/share/fonts/truetype/ibm-plex >/dev/null; \
    unzip -j -o "$tmp/mono.zip" "ibm-plex-mono/fonts/complete/ttf/*.ttf" -d /usr/share/fonts/truetype/ibm-plex >/dev/null; \
    rm -rf "$tmp"; \
    fc-cache -f >/dev/null; \
    fc-match "IBM Plex Sans" | grep -qi "plex"

RUN adduser --disabled-password --gecos "" desktop

# The base is Debian; the OS is Tiaki. Nothing user-facing says Debian:
# os-release drives Settings > About, fastfetch, and every app that asks.
RUN printf '%s\n' \
        'PRETTY_NAME="Tiaki"' \
        'NAME="Tiaki"' \
        'ID=tiaki' \
        'ID_LIKE=debian' \
        'VERSION="0.1 (dev)"' \
        'VERSION_ID="0.1"' \
        'HOME_URL="https://github.com/Daviduche03/tiaki"' \
        'BUG_REPORT_URL="https://github.com/Daviduche03/tiaki/issues"' \
        > /usr/lib/os-release \
    && ln -sf ../usr/lib/os-release /etc/os-release

# Toolchain the hand-install used to be: Node 26 (Debian trixie's node is too
# old for T3 Code), pnpm, and a pinned opencode. Arch-aware: amd64 and arm64.
ARG NODE_VERSION=26.12.1
ARG PNPM_VERSION=12.14.0
ARG OPENCODE_VERSION=2.2.5
RUN set -eu; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
        amd64) node_arch=x64; oc_arch=x64;; \
        arm64) node_arch=arm64; oc_arch=arm64;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1;; \
    esac; \
    node_dir="/opt/node-v${NODE_VERSION}-linux-${node_arch}"; \
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" -o /tmp/node.tar.xz; \
    tar -xJf /tmp/node.tar.xz -C /opt/; rm /tmp/node.tar.xz; \
    ln -s "$node_dir/bin/node" /usr/local/bin/node; \
    ln -s "$node_dir/bin/npm" /usr/local/bin/npm; \
    ln -s "$node_dir/bin/npx" /usr/local/bin/npx; \
    "$node_dir/bin/npm" i -g --no-audit --no-fund "pnpm@${PNPM_VERSION}"; \
    ln -s "$node_dir/bin/pnpm" /usr/local/bin/pnpm; \
    node --version; pnpm --version; \
    curl -fsSL "https://github.com/anomalyco/opencode/releases/download/v${OPENCODE_VERSION}/opencode-linux-${oc_arch}.tar.gz" -o /tmp/opencode.tgz; \
    tar -xzf /tmp/opencode.tgz -C /usr/local/bin/ opencode; rm /tmp/opencode.tgz; \
    chmod 755 /usr/local/bin/opencode; \
    opencode --version

# T3 Code fork (branch tiaki/extra-mcp-servers): source plus installed
# dependencies, owned by the user that runs it. pnpm install needs network
# at build time, like apt above.
COPY t3code-upstream /opt/t3code-upstream
RUN cd /opt/t3code-upstream && pnpm install --prefer-offline \
    && find /opt/t3code-upstream -name "._*" -not -path "*/node_modules/*" -delete \
    && chown -R desktop:desktop /opt/t3code-upstream

COPY --from=builder /out/agentd /usr/local/bin/agentd
COPY --from=builder /out/tiaki-ui-native /usr/local/bin/tiaki-ui-native
COPY --from=builder /out/tiaki-shell /usr/local/bin/tiaki-shell
COPY --from=builder /out/sched-launch /usr/local/bin/sched-launch
COPY --from=builder /out/acp-run /usr/local/bin/acp-run
COPY runtime/skills /usr/share/tiaki/skills
COPY desktop/desktop.json /usr/share/tiaki/desktop.json
COPY desktop/icons /usr/share/tiaki/icons
COPY desktop/wallpapers /usr/share/tiaki/wallpapers
COPY desktop/wayland/tiaki-wallpaper.sh /usr/local/bin/tiaki-wallpaper
COPY desktop/wayland/sway.conf /etc/tiaki/sway.conf
COPY desktop/wayland/wayvnc.conf /etc/tiaki/wayvnc.conf
COPY desktop/wayland/btop/btop.conf /home/desktop/.config/btop/btop.conf
COPY desktop/wayland/gtk-settings.ini /home/desktop/.config/gtk-3.0/settings.ini
COPY desktop/wayland/gtk-settings.ini /home/desktop/.config/gtk-4.0/settings.ini
COPY desktop/wayland/gtk.css /home/desktop/.config/gtk-3.0/gtk.css
COPY desktop/wayland/foot.ini /etc/tiaki/foot.ini
COPY desktop/wayland/terminal-demo.sh /usr/share/tiaki/terminal-demo.sh
COPY desktop/wayland/demo.rs /home/desktop/demo.rs
COPY desktop/wayland/bashrc /home/desktop/.bashrc
COPY desktop/wayland/run-wayland.sh /usr/local/bin/run-wayland
COPY desktop/wayland/tiaki-session /usr/local/bin/tiaki-session
COPY desktop/systemd/tiaki-firstboot.sh /usr/local/bin/tiaki-firstboot
COPY desktop/systemd/tiaki-t3.sh /usr/local/bin/tiaki-t3
COPY desktop/systemd/tiaki-firstboot.service desktop/systemd/tiaki-ui-serve.service desktop/systemd/tiaki-session.service desktop/systemd/tiaki-t3.service /etc/systemd/system/
COPY models /usr/share/tiaki/models
COPY plugins /usr/share/tiaki/plugins
COPY image/examples /usr/share/tiaki/image-examples

RUN chown -R desktop:desktop /home/desktop \
    && chmod 0755 /usr/local/bin/agentd /usr/local/bin/tiaki-ui-native \
        /usr/local/bin/tiaki-shell /usr/local/bin/run-wayland /usr/local/bin/tiaki-session /usr/local/bin/tiaki-wallpaper \
        /usr/local/bin/tiaki-firstboot /usr/local/bin/tiaki-t3 /usr/share/tiaki/terminal-demo.sh \
    && mkdir -p /var/lib/tiaki \
    && systemctl enable tiaki-firstboot.service tiaki-ui-serve.service tiaki-session.service tiaki-t3.service

ENV TIAKI_HOME=/var/lib/tiaki
EXPOSE 5900 5733
# PID 1 is systemd: firstboot seeds state and delegates cgroups, ui-serve
# answers the tree API, and the session unit runs Sway as the desktop user.
# run-wayland.sh remains for legacy direct launches.
STOPSIGNAL SIGRTMIN+3
ENTRYPOINT ["/sbin/init"]
