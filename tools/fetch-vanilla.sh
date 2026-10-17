#!/usr/bin/env bash
# Download the pinned official bundler and run Mojang's report provider.
set -euo pipefail
source "$(dirname "$0")/common.sh"
fetch https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json \
    "$VANILLA/version.json" 11f1aefe473eefba0af62e3286c33b692807b2eb
URL=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["downloads"]["server"]["url"])' "$VANILLA/version.json")
fetch "$URL" "$VANILLA/server.jar" 33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c
cd "$VANILLA"
"$JAVA" -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports --output generated
