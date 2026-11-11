# Builds Tiaki's window manager: our config crate on ShojiWM's Rust runtime.
#
# The crate is standalone (`[workspace]`) and depends on ShojiWM by pinned
# revision. `Cargo.lock` is committed, so the transitive smithay/winit pair is
# fixed; `--locked` refuses to drift.

FROM debian:trixie-slim AS build

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config ca-certificates curl git clang libclang-dev \
      libwayland-dev libxkbcommon-dev libinput-dev libudev-dev libseat-dev \
      libdrm-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev \
      libpixman-1-dev libfontconfig1-dev libfreetype-dev \
      libxcb1-dev libxcb-composite0-dev libxcb-render0-dev libxcb-shm0-dev \
      libxcb-xfixes0-dev libxcb-util-dev libxcb-image0-dev \
      libxcb-cursor-dev libxcb-render-util0-dev libx11-xcb-dev libxcursor-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release --locked

# Only the binary leaves the build. Assets are installed by the payload.
FROM scratch
COPY --from=build /build/target/release/tiaki-wm /tiaki-wm
