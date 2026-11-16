#!/usr/bin/env bash
# The job summary (docs/apple/plan.md §4, step 8): how many screenshots, the
# artifact's link, and the tests that failed.
#   summary.sh <platform> <shots dir> <xcodebuild log> <artifact url>
set -uo pipefail
platform="$1"; shots="$2"; log="$3"; url="${4:-}"
count="$(find "$shots" -name '*.png' 2>/dev/null | wc -l | tr -d ' ')"
{
    echo "## $platform"
    echo
    if [[ -n "$url" ]]; then
        echo "$count screenshot(s): [shots-$platform]($url)"
    else
        echo "$count screenshot(s)"
    fi
    find "$shots" -name '*.png' -exec basename {} \; 2>/dev/null | sort | sed 's/^/- /'
    failed="$(grep -E "Test Case .* failed|error: -\[|\) failed after|recorded an issue" "$log" 2>/dev/null | head -n 20)"
    if [[ -n "$failed" ]]; then
        echo
        echo "### Failed"
        echo '```'
        echo "$failed"
        echo '```'
    fi
} >> "${GITHUB_STEP_SUMMARY:-/dev/stdout}"
