# GNOME Shell test image: a headless GNOME Shell on Wayland with Xwayland, a system and
# session D-Bus, software rendering, and the libraries the live fixture app loads. The base
# picks the GNOME version: ubuntu:24.04 has GNOME 46, fedora:41 47, fedora:42 48, and
# fedora:43 49. The binaries under test are built elsewhere and mounted in.
ARG BASE=ubuntu:24.04
FROM ${BASE}

RUN if command -v apt-get >/dev/null; then \
        apt-get update \
        && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            gnome-shell gnome-shell-common xwayland dbus dbus-user-session dconf-cli \
            dconf-gsettings-backend libglib2.0-bin libgl1-mesa-dri procps ca-certificates \
            libwayland-client0 libwayland-cursor0 libxkbcommon0 libxkbcommon-x11-0 libxcursor1 \
            libxi6 libxrandr2 libx11-xcb1 fonts-dejavu-core netpbm \
        && rm -rf /var/lib/apt/lists/*; \
    else \
        dnf install -y --setopt=install_weak_deps=False \
            gnome-shell xorg-x11-server-Xwayland dbus-daemon dconf glib2 mesa-dri-drivers \
            procps-ng libwayland-client libwayland-cursor libxkbcommon libxkbcommon-x11 \
            libXcursor libXi libXrandr dejavu-sans-fonts netpbm-progs findutils \
        && dnf clean all; \
    fi

# GNOME Shell takes /run/systemd/seats as a sign that logind runs; it doesn't here.
RUN rm -rf /run/systemd/seats && useradd --create-home --uid 1000 snappy 2>/dev/null \
    || useradd --create-home snappy

WORKDIR /work
