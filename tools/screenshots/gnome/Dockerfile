# The GNOME 49 test image (tools/live/gnome) plus the apps the desktop animation shows, and
# the tools that turn its frames into an animated WebP.
ARG BASE=snappy-gnome:49
FROM ${BASE}

RUN dnf install -y --setopt=install_weak_deps=False \
        gnome-text-editor gnome-console git-core python3 \
        adwaita-sans-fonts adwaita-mono-fonts ImageMagick libwebp-tools \
    && dnf clean all
