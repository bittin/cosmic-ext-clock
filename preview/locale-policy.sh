#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only

catalog_has_messages() {
    local catalog="$1"
    awk '
        /^[[:space:]]*($|#)/ { next }
        /^[[:alnum:]_-]+[[:space:]]*=/ { found=1; exit }
        END { exit !found }
    ' "$catalog"
}

locale_capture_changed() {
    local locale="$1" differs="$2"
    if ((differs == 0)); then
        printf 'error: locale %s produced only pixel-identical English screenshots\n' "$locale" >&2
        return 1
    fi
}
