#!/usr/bin/env bash
# Drives the real window on a headless display, one picture per scene: KDE's
# kwin_wayland when installed, else GNOME's mutter (as in CI; with no GPU set
# GSK_RENDERER=cairo). A scene passes when it exits cleanly and left a picture.
#
#   cargo build -p fob-gtk && crates/fob-gtk/scenes.sh [out-dir]
set -uo pipefail

SCENES=${SCENES:-"first-run start locked home receive send pay-code settings add-wallet coins contacts request split profile node prices walk"}
cd "$(dirname "$0")/../.."
out=${1:-$(mktemp -d)}
mkdir -p "$out"
bin=$PWD/target/debug/fob
sock=fob-test-$$
if command -v kwin_wayland >/dev/null; then
	kwin_wayland --virtual --no-lockscreen --socket="$sock" --width=900 --height=860 >/dev/null 2>"$out/display.log" &
else
	mutter --headless --wayland --no-x11 --wayland-display="$sock" --virtual-monitor 900x860 >/dev/null 2>"$out/display.log" &
fi
display=$!
trap 'kill "$display" 2>/dev/null' EXIT
sleep 3
fail=0
for scene in $SCENES; do
	dir=$(mktemp -d)
	WAYLAND_DISPLAY=$sock GDK_BACKEND=wayland FOB_SCENE_DIR=$dir FOB_SCENE=$scene FOB_SCENE_PNG=$out/$scene.png timeout 60 "$bin" 2>"$out/$scene.log"
	code=$?
	rm -rf "$dir"
	if [ $code -eq 0 ] && [ -s "$out/$scene.png" ]; then
		echo "ok   $scene"
	else
		echo "FAIL $scene (exit $code)"
		tail -3 "$out/$scene.log"
		fail=1
	fi
done
echo "Pictures in $out"
exit $fail
