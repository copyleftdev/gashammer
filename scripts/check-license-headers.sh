#!/usr/bin/env bash
# Check that all .rs files contain the BSL-1.1 license header.
# Usage: ./scripts/check-license-headers.sh
# Exit code: 0 if all files pass, 1 if any are missing the header.

set -euo pipefail

EXPECTED="// Copyright (c) 2025-present Don Johnson"

FAILED=()

while IFS= read -r -d '' file; do
    if ! head -1 "$file" | grep -qF "$EXPECTED"; then
        FAILED+=("$file")
    fi
done < <(find crates/ -name '*.rs' -not -path '*/target/*' -print0)

# Also check build.rs files
while IFS= read -r -d '' file; do
    if ! head -1 "$file" | grep -qF "$EXPECTED"; then
        FAILED+=("$file")
    fi
done < <(find crates/ -name 'build.rs' -not -path '*/target/*' -print0)

if [ ${#FAILED[@]} -gt 0 ]; then
    echo "ERROR: The following files are missing the BSL-1.1 license header:"
    echo ""
    for f in "${FAILED[@]}"; do
        echo "  - $f"
    done
    echo ""
    echo "Expected first line: $EXPECTED"
    exit 1
fi

echo "All .rs files have the BSL-1.1 license header."
