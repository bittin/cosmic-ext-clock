#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only

set -euo pipefail
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

python3 scripts/gen-metadata.py >/dev/null

for number in 001 002 003 004 005 006 007 008; do
    grep -Fq "preview-$number.png" preview/README.md
    grep -Fq "/preview/preview-$number.png" resources/org.cosmic_utils.clock.metainfo.xml
done

screenshot_count="$(grep -cE '<screenshot( type="default")?>' resources/org.cosmic_utils.clock.metainfo.xml)"
[[ "$screenshot_count" == 8 ]] || {
    echo "expected 8 AppStream screenshots, found $screenshot_count" >&2
    exit 1
}

variant_count="$(grep -c 'variants/preview-.*-light.png' preview/README.md)"
[[ "$variant_count" == 8 ]] || {
    echo "expected 8 light preview variants in the gallery, found $variant_count" >&2
    exit 1
}

echo "preview metadata gallery passed"
