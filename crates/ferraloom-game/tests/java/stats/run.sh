#!/usr/bin/env bash
set -euo pipefail
STAT_ROOT=$(git rev-parse --show-toplevel)
STAT_VANILLA=/home/doc/code/Ferraloom/.cache/vanilla/26.3
STAT_JDK=/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1
STAT_CLASSES=/home/doc/code/Ferraloom/.cache/tools/stats-vectors
STAT_CP="$STAT_VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do STAT_CP="$STAT_CP:$jar"; done < <(find "$STAT_VANILLA/libraries" "$STAT_VANILLA/tree-harness" -name '*.jar' -type f | LC_ALL=C sort)
mkdir -p "$STAT_CLASSES"
"$STAT_JDK/bin/javac" --release 25 -cp "$STAT_CP" -d "$STAT_CLASSES" "$STAT_ROOT/crates/ferraloom-game/tests/java/stats/StatsVectors.java"
cd "$STAT_VANILLA"
"$STAT_JDK/bin/java" -Xmx256M -Dnet.bytebuddy.experimental=true -XX:+EnableDynamicAgentLoading -cp "$STAT_CLASSES:$STAT_CP" StatsVectors "$STAT_ROOT/crates/ferraloom-game/src/stats/vanilla.json.gz"
