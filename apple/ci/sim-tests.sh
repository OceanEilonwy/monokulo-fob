#!/usr/bin/env bash
# Boots a simulator and runs one iOS UI-test scheme on it, with a time limit
# on each, and starts over once (fresh boot, then the tests) when either one
# hangs. A stuck simulator then costs one retry instead of a manual re-run
# (run 43844445331: simctl boot hung for 12.5 minutes on the iPad job).
#
#   sim-tests.sh <iPhone|iPad> <scheme> <result.xcresult> [build settings...]
#
# Run in apple/, after xcodegen generate. Limits, in minutes, from the
# environment: BOOT_MINUTES (default 10; a boot takes 3-5) and TEST_MINUTES
# (default 35, the build included). run-tests.sh still runs the tests once
# more on its own when the simulator, not a test, failed.
set -uo pipefail
family="$1"; scheme="$2"; result="$3"; shift 3
boot_limit=$(( ${BOOT_MINUTES:-10} * 60 ))
test_limit=$(( ${TEST_MINUTES:-35} * 60 ))
here="$(cd "$(dirname "$0")" && pwd)"

# within <seconds> <command...>: runs the command, killing it (and its
# children) after <seconds>. Exit 124 means it ran out of time. macOS has no
# timeout(1), so this is a watchdog in bash.
within() {
    local limit="$1"; shift
    "$@" &
    local pid=$! waited=0
    while kill -0 "$pid" 2>/dev/null; do
        if (( waited >= limit )); then
            echo "::warning::$1 still running after $(( limit / 60 )) min: stopping it"
            pkill -TERM -P "$pid" 2>/dev/null; kill -TERM "$pid" 2>/dev/null
            sleep 10
            pkill -KILL -P "$pid" 2>/dev/null; kill -KILL "$pid" 2>/dev/null
            wait "$pid" 2>/dev/null
            return 124
        fi
        sleep 5; waited=$(( waited + 5 ))
    done
    wait "$pid"
}

boot() {
    xcrun simctl boot "$udid" || true
    xcrun simctl bootstatus "$udid" -b
}

for attempt in 1 2; do
    udid="$("$here/pick-simulator.sh" "$family")" || { echo "::error::No $family simulator"; exit 1; }
    start=$SECONDS
    within "$boot_limit" boot
    status=$?
    echo "$family simulator boot: $(( SECONDS - start ))s (attempt $attempt)"
    if (( status == 0 )); then
        xcrun simctl status_bar "$udid" override --time 9:41 --batteryState charged \
            --batteryLevel 100 --wifiBars 3 --cellularBars 4 || true
        within "$test_limit" "$here/run-tests.sh" "$scheme" "id=$udid" "$result" "$@"
        status=$?
        (( status != 124 )) && exit "$status"
        echo "::warning::The tests hung (attempt $attempt)"
    else
        echo "::warning::The $family simulator did not boot in $(( boot_limit / 60 )) min (attempt $attempt)"
    fi
    # Start over from a clean simulator.
    xcrun simctl shutdown all >/dev/null 2>&1 || true
    xcrun simctl erase "$udid" >/dev/null 2>&1 || true
done
echo "::error::The $family simulator hung twice"
exit 1
