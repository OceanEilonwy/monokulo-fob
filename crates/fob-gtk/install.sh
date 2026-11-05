#!/usr/bin/env bash
# Builds Fob for the desktop and installs it with its menu entry and icon.
#
#   crates/fob-gtk/install.sh                     for you alone (~/.local)
#   PREFIX=/usr/local crates/fob-gtk/install.sh   somewhere else
#   crates/fob-gtk/install.sh --uninstall         takes it out again; wallets stay
#
# Needs Rust and GTK 4.18 and libadwaita 1.7 or newer.
set -euo pipefail

cd "$(dirname "$0")/../.."
PREFIX=${PREFIX:-$HOME/.local}
ID=com.monokulo.Fob
BIN=$PREFIX/bin/fob
DESKTOP=$PREFIX/share/applications/$ID.desktop
ICON=$PREFIX/share/icons/hicolor/scalable/apps/$ID.svg

if [ "${1:-}" = "--uninstall" ]; then
	rm -f "$BIN" "$DESKTOP" "$ICON"
	echo "Removed. Your wallets are still in ${XDG_DATA_HOME:-$HOME/.local/share}/fob."
	exit 0
fi

cargo build --release --locked -p fob-gtk
install -Dm755 target/release/fob "$BIN"
sed "s|^Exec=fob|Exec=$BIN|" crates/fob-gtk/data/$ID.desktop | install -Dm644 /dev/stdin "$DESKTOP"
install -Dm644 crates/fob-gtk/data/$ID.svg "$ICON"
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
echo "Installed $BIN."
