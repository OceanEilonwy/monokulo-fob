#!/usr/bin/env bash
# Draws Connect to Fob's screens (ConnectScreenshotTest) on an emulator and
# pulls one PNG per screen into $1 (default build/screenshots), to hold
# against the designs. Starts its own read-only emulator of $DEVICE_NAME
# (default Shots_phone) on port $PORT, so one already running is left alone.
#
#   JAVA_HOME=/opt/android-studio/jbr scripts/screenshots.sh /tmp/shots
set -euo pipefail
cd "$(dirname "$0")/.."

DEVICE_NAME=${DEVICE_NAME:-Shots_phone}
PORT=${PORT:-5600}
OUT=$(realpath -m "${1:-build/screenshots}")
SDK=${ANDROID_HOME:-$HOME/Android/Sdk}
ADB="$SDK/platform-tools/adb"
export ANDROID_SERIAL=emulator-$PORT

"$SDK/emulator/emulator" -avd "$DEVICE_NAME" -port "$PORT" -read-only -no-window -no-audio -no-boot-anim -no-snapshot-save >/dev/null 2>&1 &
EMULATOR=$!
trap 'kill $EMULATOR 2>/dev/null || true' EXIT
"$ADB" wait-for-device
until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done

# Installed and run by hand: connectedAndroidTest uninstalls the app after, and the PNGs with it.
./gradlew -q installDebug installDebugAndroidTest
"$ADB" shell am instrument -w -e class com.monokulo.fob.ConnectScreenshotTest com.monokulo.fob.debug.test/androidx.test.runner.AndroidJUnitRunner
rm -rf "$OUT"
mkdir -p "$OUT"
"$ADB" pull /sdcard/Android/data/com.monokulo.fob.debug/files/screenshots/. "$OUT" >/dev/null
ls "$OUT"
