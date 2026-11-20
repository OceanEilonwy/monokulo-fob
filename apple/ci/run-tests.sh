#!/usr/bin/env bash
# Runs one platform's UI-test scheme (docs/apple/plan.md §4, step 7), once
# more only when the simulator failed and no test did (the till's list:
# "The test runner hung before establishing connection" and the like, or
# run 44645541687's iPad: "Failed to install or launch the test runner ...
# Invalid device state ... (ipc/mig) server died"; run 44650412186's iPad:
# "The test runner crashed while preparing to run tests"). Any failure in
# which the build finished but no test case ever started counts too.
#
#   run-tests.sh <scheme> <destination> <result.xcresult> [build settings...]
#
# Run in apple/, after xcodegen generate. The log is <result>.log.
set -uo pipefail
scheme="$1"; destination="$2"; result="$3"; shift 3
log="$result.log"

run() {
    rm -rf "$result"
    xcodebuild test \
        -project Fob.xcodeproj -scheme "$scheme" \
        -destination "$destination" \
        -resultBundlePath "$result" \
        -parallel-testing-enabled NO \
        "$@" 2>&1 | tee "$log"
    return "${PIPESTATUS[0]}"
}

if run "$@"; then exit 0; fi
simulator_failed() {
    grep -qE "before establishing connection|never began executing|Restarting after unexpected exit|Setting up automation session.*timed out|lost connection to the test|Early unexpected exit|Unable to boot|failed to boot|Failed to install or launch the test runner|Invalid device state|server died|crashed while preparing to run tests" "$log" && return 0
    # The build was fine but no test ever started.
    ! grep -qE "BUILD FAILED|the build failed|Test Case '.*' started" "$log"
}
if simulator_failed && ! grep -qE "\) failed after|Test Case .* failed|recorded an issue" "$log"; then
    echo "::warning::The simulator failed, not a test: running the tests once more"
    xcrun simctl shutdown all >/dev/null 2>&1 || true
    # A simulator whose services died needs a clean slate, not just a reboot;
    # boot it again here so the screenshots keep sim-tests.sh's status bar.
    udid="${destination#id=}"
    if [[ "$udid" != "$destination" ]]; then
        xcrun simctl erase "$udid" >/dev/null 2>&1 || true
        xcrun simctl boot "$udid" >/dev/null 2>&1 || true
        xcrun simctl bootstatus "$udid" -b >/dev/null 2>&1 || true
        xcrun simctl status_bar "$udid" override --time 9:41 --batteryState charged \
            --batteryLevel 100 --wifiBars 3 --cellularBars 4 >/dev/null 2>&1 || true
    fi
    run "$@"
    exit $?
fi
exit 1
