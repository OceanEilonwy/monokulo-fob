#!/usr/bin/env bash
# Builds Fob as an AppImage, into dist/. Run it on Debian 13 (as the release
# workflow does): it has the oldest GTK and libadwaita Fob can use, and the
# AppImage runs where glibc is at least as new as Debian's, 2.41.
#
#   apt-get install build-essential curl file pkg-config desktop-file-utils \
#     libgtk-4-dev libadwaita-1-dev librsvg2-common adwaita-icon-theme gsettings-desktop-schemas
#   crates/fob-gtk/appimage.sh
#
# The tools are fixed versions, checked by hash: they end up inside a wallet.
set -euo pipefail

cd "$(dirname "$0")/../.."
ID=com.monokulo.Fob
DATA=crates/fob-gtk/data
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
TARGET=${CARGO_TARGET_DIR:-target}
WORK=$TARGET/appimage
APPDIR=$PWD/$WORK/AppDir

fetch() { # url sha256 file
	[ -f "$WORK/$3" ] || curl -fsSL -o "$WORK/$3" "$1"
	echo "$2  $WORK/$3" | sha256sum -c --quiet -
	chmod +x "$WORK/$3"
}

mkdir -p "$WORK" dist
rm -rf "$APPDIR"
fetch https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
	c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d linuxdeploy
fetch https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
	ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 appimagetool
fetch https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
	2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d runtime
# So they run without FUSE, as in a container.
export APPIMAGE_EXTRACT_AND_RUN=1

cargo build --release --locked -p fob-gtk

# The picture loader for GTK's symbolic icons, which GTK 4.18 can't draw alone.
multiarch=$(gcc -print-multiarch)
pixbuf=/usr/lib/$multiarch/gdk-pixbuf-2.0
loader=$pixbuf/2.10.0/loaders/libpixbufloader_svg.so
[ -f "$loader" ] || loader=$pixbuf/2.10.0/loaders/libpixbufloader-svg.so

"$WORK/linuxdeploy" --appdir "$APPDIR" \
	--executable "$TARGET/release/fob" \
	--library "$loader" \
	--desktop-file "$DATA/$ID.desktop" \
	--icon-file "$DATA/$ID.svg" \
	--custom-apprun crates/fob-gtk/data/AppRun

# Named by file alone, so the loader is found inside the AppImage wherever it's mounted.
"$pixbuf/gdk-pixbuf-query-loaders" "$loader" | sed "s|\"$(dirname "$loader")/|\"|" >"$APPDIR/usr/lib/gdk-pixbuf-loaders.cache"

# What the window needs and a desktop might not have.
mkdir -p "$APPDIR/usr/share/glib-2.0/schemas" "$APPDIR/usr/share/icons/Adwaita"
cp /usr/share/glib-2.0/schemas/org.gtk.gtk4.*.gschema.xml /usr/share/glib-2.0/schemas/org.gnome.desktop.interface.gschema.xml \
	/usr/share/glib-2.0/schemas/org.gnome.desktop.enums.xml "$APPDIR/usr/share/glib-2.0/schemas/" 2>/dev/null || true
glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas"
cp /usr/share/icons/Adwaita/index.theme "$APPDIR/usr/share/icons/Adwaita/"
cp -r /usr/share/icons/Adwaita/symbolic "$APPDIR/usr/share/icons/Adwaita/"
install -Dm644 "$DATA/$ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$ID.metainfo.xml"

ARCH=x86_64 VERSION=$VERSION "$WORK/appimagetool" --no-appstream --runtime-file "$WORK/runtime" "$APPDIR" "dist/fob-$VERSION-x86_64.AppImage"
echo "Built dist/fob-$VERSION-x86_64.AppImage."
