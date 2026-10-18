#!/usr/bin/env bash
set -euo pipefail
ADV_ROOT=$(git rev-parse --show-toplevel)
ADV_VANILLA=/home/doc/code/Ferraloom/.cache/vanilla/26.3
ADV_JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
ADV_CLASSES=/home/doc/code/Ferraloom/.cache/tools/advancement-vectors
ADV_CP="$ADV_VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do ADV_CP="$ADV_CP:$jar"; done < <(find "$ADV_VANILLA/libraries" "$ADV_VANILLA/tree-harness" -name '*.jar' -type f | LC_ALL=C sort)
mkdir -p "$ADV_CLASSES"
"$ADV_JDK/bin/javac" --release 25 -cp "$ADV_CP" -d "$ADV_CLASSES" "$ADV_ROOT/crates/ferraloom-game/tests/java/advancements/AdvancementVectors.java"
cd "$ADV_VANILLA"
"$ADV_JDK/bin/java" -Xmx2g -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$ADV_CLASSES:$ADV_CP" AdvancementVectors "$ADV_ROOT/crates/ferraloom-game/src/advancements/vanilla.json.gz"
