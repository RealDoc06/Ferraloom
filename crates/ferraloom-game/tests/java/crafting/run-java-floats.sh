#!/usr/bin/env bash
set -euo pipefail
RECIPE_ROOT=$(git rev-parse --show-toplevel)
RECIPE_JDK=${JAVA_HOME:-/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1}
RECIPE_CLASSES=/home/doc/code/Ferraloom/.cache/tools/java-float-values
mkdir -p "$RECIPE_CLASSES"
"$RECIPE_JDK/bin/javac" --release 25 -d "$RECIPE_CLASSES" "$RECIPE_ROOT/crates/ferraloom-game/tests/java/crafting/FloatValues.java"
"$RECIPE_JDK/bin/java" -cp "$RECIPE_CLASSES" FloatValues "$RECIPE_ROOT/crates/ferraloom-util/src/java_float_vectors.rs"
