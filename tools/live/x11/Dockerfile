# Linux X11 development image: the Rust toolchain plus a headless X server (Xorg on the
# dummy driver), the Xfwm4 window manager, and a D-Bus session for the adapter's
# integration tests.
FROM debian:stable-slim

ARG RUST_TOOLCHAIN=1.99.1

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential pkg-config procps \
        xserver-xorg-core xserver-xorg-video-dummy xfwm4 xfconf dbus dbus-x11 \
        x11-xserver-utils x11-xkb-utils x11-utils xdotool \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    CARGO_TARGET_DIR=/target

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN" \
        --component clippy --component rustfmt \
    && rustc --version

# The live harness: the libraries its winit fixture app loads at run time, and `xwd` with
# netpbm for screenshots of failed scenarios.
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        libxkbcommon-x11-0 libxcursor1 libxi6 libxrandr2 x11-apps netpbm \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
