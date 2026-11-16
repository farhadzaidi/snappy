# Packages the tree tools/release/linux.sh stages; nothing is built here. linux.sh passes
# snappy_version, snappy_license, and snappy_tree.

# Prebuilt binaries: no debug package, stripping, build-id links, or other post-install steps.
%global debug_package %{nil}
%global __os_install_post %{nil}
%global _build_id_links none

Name:           snappy-app
Version:        %{snappy_version}
Release:        1
Summary:        Snappy arranges windows into zones
License:        %{snappy_license}
URL:            https://github.com/farhadzaidi/snappy
Requires:       webkit2gtk4.1
Requires:       gtk3
# Snappy's own files provide nothing other packages should depend on.
AutoProv:       no

%description
Pick a layout for each display, then drag a window with a key held, or press a
shortcut, and it snaps into place. Snappy also swaps windows, moves focus between
them, and keeps workspaces of windows to switch between. It never uses the
network, and never reads window titles, window contents, or what you type.

Works on GNOME and KDE Plasma on Wayland, and on X11. The commands are snappy,
which runs in the background, and snappy-app, its window.

%install
cp -a %{snappy_tree}/. %{buildroot}/

%files
/usr/bin/snappy
/usr/bin/snappy-app
/usr/share/applications/io.github.farhadzaidi.Snappy.desktop
/usr/share/icons/hicolor/256x256/apps/io.github.farhadzaidi.Snappy.png
/usr/share/icons/hicolor/scalable/apps/io.github.farhadzaidi.Snappy.svg
%dir /usr/share/licenses/snappy-app
%license /usr/share/licenses/snappy-app/LICENSE.md
