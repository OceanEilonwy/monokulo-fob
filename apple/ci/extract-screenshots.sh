#!/usr/bin/env bash
# Turns an xcresult's screenshot attachments into PNGs named by attachment:
# a UI test that attaches "T-12--oled" (docs/apple/plan.md §5) gives
# <outdir>/T-12--oled.png.
#
#   extract-screenshots.sh <result.xcresult> <outdir>
#
# Uses `xcrun xcresulttool export attachments` (Xcode 16 and later), which
# writes every attachment with a manifest.json naming each exported file's
# attachment ("suggestedHumanReadableName": "T-12--oled_0_<UUID>.png").
set -euo pipefail

result="${1:?usage: $0 <result.xcresult> <outdir>}"
out="${2:?usage: $0 <result.xcresult> <outdir>}"
if [[ ! -d "$result" ]]; then
    echo "no result bundle at $result" >&2
    mkdir -p "$out"
    exit 0
fi

raw="$(mktemp -d)"
trap 'rm -rf "$raw"' EXIT
mkdir -p "$out"
xcrun xcresulttool export attachments --path "$result" --output-path "$raw"

python3 - "$raw" "$out" <<'PY'
import json, os, re, shutil, sys

raw, out = sys.argv[1], sys.argv[2]
manifest = json.load(open(os.path.join(raw, "manifest.json")))
count = 0
for test in manifest:
    for a in test.get("attachments", []):
        exported = a.get("exportedFileName")
        if not exported:
            continue
        name = a.get("suggestedHumanReadableName") or exported
        stem, ext = os.path.splitext(name)
        # Drop what xcresulttool adds to the attachment's own name.
        stem = re.sub(r"_\d+_[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$", "", stem)
        stem = re.sub(r"[^A-Za-z0-9._-]+", "_", stem) or "attachment"
        ext = (os.path.splitext(exported)[1] or ext or ".png").lower()
        if ext not in (".png", ".jpg", ".jpeg", ".heic"):
            continue
        target = os.path.join(out, stem + ext)
        n = 1
        while os.path.exists(target):
            n += 1
            target = os.path.join(out, f"{stem}-{n}{ext}")
        shutil.copyfile(os.path.join(raw, exported), target)
        print(f"{os.path.basename(target)}  <-  {test.get('testIdentifier', '?')}")
        count += 1
print(f"{count} screenshot(s) in {out}")
PY
