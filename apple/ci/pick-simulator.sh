#!/usr/bin/env bash
# Prints the udid of the simulator a job tests on (docs/apple/plan.md §4):
#   pick-simulator.sh iPhone   the newest iPhone (not a foldable, not a Plus or
#                              Max) on the newest iOS 27 runtime
#   pick-simulator.sh iPad     an iPad on the same runtime, the 11-inch Air
#                              (1180 x 820, as the design boards) when there is one
set -euo pipefail
family="${1:?usage: $0 iPhone|iPad}"
xcrun simctl list devices available -j | python3 -c '
import json, re, sys
family = sys.argv[1]
devices = json.load(sys.stdin)["devices"]
def version(runtime):
    return tuple(int(n) for n in re.findall(r"\d+", runtime.rsplit(".", 1)[-1]))
runtimes = sorted((r for r in devices if ".iOS-27-" in r or r.endswith(".iOS-27")), key=version, reverse=True)
if not runtimes:
    runtimes = sorted((r for r in devices if ".iOS-" in r), key=version, reverse=True)
    print(f"::warning::No iOS 27 runtime; using {runtimes[0] if runtimes else None}", file=sys.stderr)
def score(name):
    if family == "iPad":
        for i, want in enumerate(["iPad Air 11", "iPad Air (11", "iPad (A16)", "iPad Pro 11", "iPad Air"]):
            if want in name:
                return i
        return 10
    if re.search(r"Plus|Max|Duo|Fold|Flip", name):
        return 20
    numbers = re.findall(r"\d+", name)
    # Newest first: a higher model number scores lower.
    return 10 - (int(numbers[0]) / 100 if numbers else 0) - (0.5 if name.endswith(" Pro") else 0)
for runtime in runtimes:
    found = [d for d in devices[runtime] if d["name"].startswith(family) and not re.search(r"Duo|Fold|Flip", d["name"])]
    if found:
        best = min(found, key=lambda d: score(d["name"]))
        print(f"{best['udid']}")
        print(f"{family}: {best['name']} on {runtime}", file=sys.stderr)
        sys.exit(0)
sys.exit(1)' "$family"
