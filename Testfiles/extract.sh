#!/usr/bin/env bash
# Extracts every zip-based slice file in this directory (.gcode.3mf, .sl1,
# .sl1s) into unziped/<original filename, including extension>/, for manual
# inspection of the container contents. Safe to re-run; overwrites in place.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p unziped

for f in *.gcode.3mf *.sl1 *.sl1s; do
    [ -e "$f" ] || continue
    rm -rf "unziped/$f"
    unzip -o -q "$f" -d "unziped/$f"
    echo "extracted $f -> unziped/$f/"
done
