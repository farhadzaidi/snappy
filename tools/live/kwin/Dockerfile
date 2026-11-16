# A headless KWin for the KDE tests: KWin on Wayland with Xwayland, a D-Bus session, and the
# libraries the fixture app loads. The binaries under test come from the shared build
# container (tools/live/build).
ARG BASE=fedora:45
FROM ${BASE}

RUN dnf install -y --setopt=install_weak_deps=False \
        kwin-wayland xorg-x11-server-Xwayland dbus-daemon dbus-tools procps-ng \
        mesa-dri-drivers mesa-libEGL xkeyboard-config \
        libxkbcommon-x11 libXcursor libXi libXrandr \
    && dnf clean all

# kwin-session.sh runs KWin straight away here: the container is the seal.
ENV SNAPPY_KWIN_CONTAINER=1

WORKDIR /work
