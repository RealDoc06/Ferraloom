#!/usr/bin/env bash
set -euo pipefail
RECIPE_ROOT=$(git rev-parse --show-toplevel)
RECIPE_VANILLA=${FERRALOOM_VANILLA_CACHE:-/home/doc/code/Ferraloom/.cache/vanilla/26.3}
RECIPE_JDK=${JAVA_HOME:-/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1}
RECIPE_CLASSES=/home/doc/code/Ferraloom/.cache/tools/component-float-values
RECIPE_CP="$RECIPE_VANILLA/versions/26.3/server-26.3.jar"
while IFS= read -r jar; do RECIPE_CP="$RECIPE_CP:$jar"; done < <(find "$RECIPE_VANILLA/libraries" -type f -name '*.jar' | LC_ALL=C sort)
mkdir -p "$RECIPE_CLASSES"
"$RECIPE_JDK/bin/javac" --release 25 -cp "$RECIPE_CP" -d "$RECIPE_CLASSES" "$RECIPE_ROOT/crates/ferraloom-game/tests/java/crafting/FloatValues.java" "$RECIPE_ROOT/crates/ferraloom-game/tests/java/crafting/ComponentFloatValues.java"
cd "$RECIPE_VANILLA"
"$RECIPE_JDK/bin/java" -Xmx256M -cp "$RECIPE_CLASSES:$RECIPE_CP" ComponentFloatValues "$RECIPE_ROOT/crates/ferraloom-protocol/src/component/float-fixtures.json"
