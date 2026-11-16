#!/usr/bin/env bash
# Runs one platform's UI-test scheme (docs/apple/plan.md §4, step 7), once
# more only when the simulator failed and no test did (the till's list:
# "The test runner hung before establishing connection" and the like).
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
        "$@" 2>&1 | tee "$log"
    return "${PIPESTATUS[0]}"
}

if run "$@"; then exit 0; fi
if grep -qE "before establishing connection|never began executing|lost connection to the test|Early unexpected exit|Unable to boot|failed to boot" "$log" \
    && ! grep -qE "\) failed after|Test Case .* failed|recorded an issue" "$log"; then
    echo "::warning::The simulator failed, not a test: running the tests once more"
    xcrun simctl shutdown all >/dev/null 2>&1 || true
    run "$@"
    exit $?
fi
exit 1
