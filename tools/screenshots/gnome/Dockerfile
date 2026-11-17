# The GNOME 49 test image (tools/live/gnome) plus the apps the desktop screenshots show.
ARG BASE=snappy-gnome:49
FROM ${BASE}

RUN dnf install -y --setopt=install_weak_deps=False \
        gnome-text-editor nautilus gnome-console adwaita-sans-fonts adwaita-mono-fonts \
    && dnf clean all
