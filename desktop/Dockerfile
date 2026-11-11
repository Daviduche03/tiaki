# syntax=docker/dockerfile:1

FROM rust:1.85-slim-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p agentd

# Build a real Debian package from the release binary and desktop assets.
FROM debian:trixie-slim AS package
RUN apt-get update \
    && apt-get install -y --no-install-recommends dpkg-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /pkg
RUN mkdir -p /out usr/local/bin usr/share/tiaki lib/systemd/system DEBIAN
COPY --from=builder /src/target/release/agentd usr/local/bin/agentd
COPY desktop/desktop.json usr/share/tiaki/desktop.json
COPY desktop/kiosk.sh usr/share/tiaki/kiosk.sh
COPY desktop/tiaki-desktop.service lib/systemd/system/tiaki-desktop.service
COPY models usr/share/tiaki/models
COPY plugins usr/share/tiaki/plugins
COPY image/examples usr/share/tiaki/image-examples
COPY desktop/control DEBIAN/control
COPY desktop/postinst DEBIAN/postinst
COPY desktop/prerm DEBIAN/prerm
RUN chmod 0755 DEBIAN/postinst DEBIAN/prerm usr/local/bin/agentd usr/share/tiaki/kiosk.sh \
    && dpkg-deb --build /pkg /out/tiaki-desktop.deb

# The runnable desktop container.
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/agentd /usr/local/bin/agentd
COPY --from=package /out/tiaki-desktop.deb /opt/tiaki-desktop.deb
COPY desktop/desktop.json /usr/share/tiaki/desktop.json
COPY desktop/entrypoint.sh /usr/local/bin/tiaki-desktop-entrypoint
COPY models /usr/share/tiaki/models
COPY plugins /usr/share/tiaki/plugins
COPY image/examples /usr/share/tiaki/image-examples
RUN chmod 0755 /usr/local/bin/agentd /usr/local/bin/tiaki-desktop-entrypoint \
    && mkdir -p /var/lib/tiaki
ENV TIAKI_HOME=/var/lib/tiaki
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/tiaki-desktop-entrypoint"]
