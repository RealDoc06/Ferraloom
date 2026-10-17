#!/usr/bin/env bash
# Java 25 is needed only to regenerate committed facts, never for a Rust build.
set -euo pipefail
source "$(dirname "$0")/common.sh"
"$ROOT/tools/fetch-vanilla.sh"
CP="$VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do CP="$CP:$jar"; done < <(find "$VANILLA/libraries" -type f -name '*.jar' | LC_ALL=C sort)
CLASSES="$CACHE/tools/extractor-classes"
mkdir -p "$CLASSES" "$ROOT/data/generated"
"$JAVAC" --release 25 -cp "$CP" -d "$CLASSES" "$ROOT/tools/extractor/ExtractData.java"
# Keep bootstrap logs and incidental files in the shared cache.
cd "$VANILLA"
"$JAVA" -Xmx2G -cp "$CLASSES:$CP" ExtractData "$ROOT/data/generated"
for report in blocks registries packets commands; do
    cp "$VANILLA/generated/reports/$report.json" "$ROOT/data/generated/$report.json"
done
