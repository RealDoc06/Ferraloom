#!/usr/bin/env bash
# Reproduce the unobfuscated vanilla source with the pinned Vineflower release.
set -euo pipefail
source "$(dirname "$0")/common.sh"
"$ROOT/tools/fetch-vanilla.sh"
URL=https://repo.maven.apache.org/maven2/org/vineflower/vineflower/1.12.0/vineflower-1.12.0.jar
SHA=$(curl --fail --silent --show-error --location "$URL.sha1")
[[ "$SHA" =~ ^[0-9a-fA-F]{40}$ ]]
JAR="$CACHE/tools/vineflower-1.12.0.jar"
fetch "$URL" "$JAR" "$SHA"
OUTPUT="$CACHE/decompiled/26.3"
mkdir -p "$OUTPUT"
LIBS=()
while IFS= read -r jar; do LIBS+=("-e=$jar"); done < <(find "$VANILLA/libraries" -type f -name '*.jar' | LC_ALL=C sort)
"$JAVA" -jar "$JAR" -dgs=1 -rsy=1 -lit=1 -mpm=60 -thr=6 "${LIBS[@]}" \
    "$VANILLA/versions/26.3/server-26.3.jar" "$OUTPUT"
# Vineflower's archive input produces an archive; expose the source tree too.
python3 - "$OUTPUT" <<'PY'
import pathlib, sys, zipfile
out = pathlib.Path(sys.argv[1])
for archive in out.glob('*.jar'):
    with zipfile.ZipFile(archive) as z:
        z.extractall(out)
    archive.unlink()
PY
